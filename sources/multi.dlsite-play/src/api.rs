use crate::models::{DownloadToken, PurchaseWork, RawZipTree, SalesEntry, WorksResponse, ZipTree};
use crate::settings;
use aidoku::{
	alloc::{format, String, Vec},
	imports::{net::{Request, Response}, std::print},
	prelude::*,
	Result,
};
use core::str;
use spin::Mutex;

/// Serializes bootstrap + `Set-Cookie` merges within one WASM instance (multiple installed source versions still share Aidoku defaults and can race).
static PLAY_PRIME_LOCK: Mutex<()> = Mutex::new(());

pub(crate) const PLAY_REFERER: &str = "https://play.dlsite.com/";
/// dlsite-async `PlayAPI` uses plain `aiohttp.ClientSession` — no browser `Origin` / `Sec-Fetch-*` / `X-Requested-With`.
const PLAY_AIOHTTP_USER_AGENT: &str = "Python/3.12 aiohttp/3.11.16";
/// CDN / optimized assets: keep a real browser UA so hotlink rules stay happy.
const PLAY_IMAGE_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";

const PLAY_API: &str = "https://play.dlsite.com/api/v3";
const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";
/// Matches `dlsite-async` `PlayAPI.login` bootstrap order (`GET /login/` then `GET /api/authorize`).
const PLAY_LOGIN_URL: &str = "https://play.dlsite.com/login/";
/// Binds DLsite account to Play API session.
const PLAY_AUTHORIZE_URL: &str = "https://play.dlsite.com/api/authorize";

/// Cookies Play may rotate via `Set-Cookie` during bootstrap (mirrors aiohttp jar for these names only).
fn is_allowlisted_play_cookie_name(name: &str) -> bool {
	name.eq_ignore_ascii_case("XSRF-TOKEN") || name.eq_ignore_ascii_case("play_session")
}

/// After `", "`, next segment is a new cookie if it looks like `name=value` (`name` is a token).
fn is_new_cookie_after_comma_space(after_comma: &str) -> bool {
	let s = after_comma.trim_start();
	let Some(eq) = s.find('=') else {
		return false;
	};
	let name = s[..eq].trim();
	!name.is_empty()
		&& name
			.bytes()
			.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Aidoku joins multiple `Set-Cookie` with `", "`; skip commas inside `expires=Wed, 01 Jan...`.
fn split_joined_set_cookie_line(line: &str) -> Vec<&str> {
	let line = line.trim();
	if line.is_empty() {
		return Vec::new();
	}
	let bytes = line.as_bytes();
	let mut out: Vec<&str> = Vec::new();
	let mut start = 0usize;
	let mut i = 0usize;
	while i + 2 <= bytes.len() {
		if bytes[i] == b',' && bytes[i + 1] == b' ' {
			let after = &line[i + 2..];
			if is_new_cookie_after_comma_space(after) {
				out.push(line[start..i].trim());
				start = i + 2;
				i += 2;
				continue;
			}
		}
		i += 1;
	}
	out.push(line[start..].trim());
	out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn parse_set_cookie_name_value(fragment: &str) -> Option<(String, String)> {
	let first = fragment.split(';').next()?.trim();
	let (name, value) = first.split_once('=')?;
	let name = name.trim();
	let value = value.trim();
	if name.is_empty() {
		return None;
	}
	Some((String::from(name), String::from(value)))
}

fn collect_allowlisted_set_cookie_pairs(resp: &Response) -> Vec<(String, String)> {
	let raw = resp
		.get_header("Set-Cookie")
		.or_else(|| resp.get_header("set-cookie"));
	let Some(raw) = raw else {
		return Vec::new();
	};
	let mut out: Vec<(String, String)> = Vec::new();
	for line in raw.lines() {
		for piece in split_joined_set_cookie_line(line) {
			if let Some((name, value)) = parse_set_cookie_name_value(piece) {
				if is_allowlisted_play_cookie_name(&name) {
					out.push((name, value));
				}
			}
		}
	}
	out
}

fn merge_allowlisted_into_cookie_header(
	base: &str,
	updates: &[(String, String)],
) -> Option<String> {
	if updates.is_empty() {
		return None;
	}
	let mut pairs: Vec<(String, String)> = Vec::new();
	for part in base.split(';') {
		let p = part.trim();
		let Some((n, v)) = p.split_once('=') else {
			continue;
		};
		pairs.push((String::from(n.trim()), String::from(v.trim())));
	}
	let mut changed = false;
	for (uname, uval) in updates {
		let mut found = false;
		for (n, v) in pairs.iter_mut() {
			if n.eq_ignore_ascii_case(uname) {
				if v != uval {
					*v = uval.clone();
					changed = true;
				}
				found = true;
				break;
			}
		}
		if !found {
			pairs.push((uname.clone(), uval.clone()));
			changed = true;
		}
	}
	if !changed {
		return None;
	}
	pairs.sort_by(|a, b| a.0.cmp(&b.0));
	Some(
		pairs
			.into_iter()
			.map(|(n, v)| format!("{}={}", n, v))
			.collect::<Vec<_>>()
			.join("; "),
	)
}

/// Apply `Set-Cookie` updates for allowlisted names into stored `Cookie` header.
fn persist_allowlisted_play_cookies_from_response(resp: &Response) {
	let updates = collect_allowlisted_set_cookie_pairs(resp);
	if updates.is_empty() {
		return;
	}
	let Some(base) = settings::get_web_cookies() else {
		return;
	};
	let Some(new_header) = merge_allowlisted_into_cookie_header(&base, &updates) else {
		return;
	};
	settings::set_web_cookies(&new_header);
	print(format!(
		"[dlsite-play] applied Set-Cookie for {:?} (Cookie header length {})",
		updates
			.iter()
			.map(|(n, _)| n.as_str())
			.collect::<Vec<_>>(),
		new_header.len()
	));
}

fn hex_digit(b: u8) -> Option<u8> {
	match b {
		b'0'..=b'9' => Some(b - b'0'),
		b'a'..=b'f' => Some(10 + b - b'a'),
		b'A'..=b'F' => Some(10 + b - b'A'),
		_ => None,
	}
}

/// Browser stacks send `X-XSRF-TOKEN` as URL-decoded cookie value (see Laravel / axios).
fn percent_decode_cookie_value(input: &str) -> String {
	let bytes = input.as_bytes();
	let mut out: Vec<u8> = Vec::new();
	let mut i = 0usize;
	while i < bytes.len() {
		if bytes[i] == b'%' && i + 2 < bytes.len() {
			if let (Some(hi), Some(lo)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
				out.push(hi * 16 + lo);
				i += 3;
				continue;
			}
		}
		out.push(bytes[i]);
		i += 1;
	}
	String::from_utf8_lossy(&out).into_owned()
}

fn xsrf_token_for_header(cookie_header: &str) -> Option<String> {
	for part in cookie_header.split(';') {
		let p = part.trim();
		let Some((name, value)) = p.split_once('=') else {
			continue;
		};
		if name.trim().eq_ignore_ascii_case("XSRF-TOKEN") {
			return Some(percent_decode_cookie_value(value.trim()));
		}
	}
	None
}

fn accept_for_url(url: &str) -> &'static str {
	if url.contains("/api/v3/") {
		return "application/json";
	}
	if url.contains("/api/authorize") {
		return "*/*";
	}
	if url.contains("/api/") {
		return "application/json";
	}
	"*/*"
}

/// Play auth bootstrap with current cookies:
/// 1) `GET /login/` (cookies only — like aiohttp; no fake browser / XHR headers)
/// 2) `GET /api/authorize` with `Referer` only (matches dlsite-async; cookies from jar — no `X-XSRF-TOKEN`)
/// Persists **only** `XSRF-TOKEN` and `play_session` from `Set-Cookie` (like aiohttp’s jar, without blind merge).
pub(crate) fn prime_play_api_session() -> Result<()> {
	if settings::get_web_cookies().is_none() {
		return Ok(());
	}
	let _prime_guard = PLAY_PRIME_LOCK.lock();
	let login_resp = play_login_page_document_get(None)?.send()?;
	let login_status = login_resp.status_code();
	persist_allowlisted_play_cookies_from_response(&login_resp);
	let _ = login_resp.get_data();
	print(format!(
		"[dlsite-play] prime_play_api_session /login/ HTTP {}",
		login_status
	));
	if login_status >= 400 {
		print(format!(
			"[dlsite-play] prime_play_api_session /login/ non-success status={}, continuing to /api/authorize",
			login_status
		));
	}

	let resp = play_authorize_get()?.send()?;
	let status = resp.status_code();
	persist_allowlisted_play_cookies_from_response(&resp);
	let _ = resp.get_data();
	print(format!(
		"[dlsite-play] prime_play_api_session /api/authorize HTTP {}",
		status
	));
	if status == 401 {
		bail!("authorize HTTP 401: complete web login again.");
	}
	if status >= 400 {
		print(format!(
			"[dlsite-play] prime_play_api_session non-success status={}, continuing",
			status
		));
	}
	Ok(())
}

/// `GET /login/` like aiohttp: `User-Agent` + `Accept` + `Cookie` only (no Referer / Sec-Fetch / XSRF header).
fn play_login_page_document_get(cookie_override: Option<&str>) -> Result<Request> {
	const ACCEPT_HTML: &str =
		"text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
	let cookie_str = cookie_override.map(String::from).or_else(settings::get_web_cookies);
	print(format!(
		"[dlsite-play] → GET {} (aiohttp-style bootstrap /login/)",
		PLAY_LOGIN_URL
	));
	print(format!(
		"[dlsite-play]     User-Agent: {}",
		PLAY_AIOHTTP_USER_AGENT
	));
	print(format!("[dlsite-play]     Accept: {}", ACCEPT_HTML));
	match cookie_str.as_deref() {
		Some(c) if !c.is_empty() => print(format!("[dlsite-play]     Cookie: {}", c)),
		_ => print(format!(
			"[dlsite-play]     Cookie: <none stored; complete web login first>"
		)),
	}
	let mut req = Request::get(PLAY_LOGIN_URL)?
		.header("User-Agent", PLAY_AIOHTTP_USER_AGENT)
		.header("Accept", ACCEPT_HTML);
	if let Some(ref c) = cookie_str {
		req = req.header("Cookie", c.as_str());
	}
	Ok(req)
}

/// `GET /api/authorize` with `Referer` only — matches dlsite-async `PlayAPI.login` (cookie jar; no `X-XSRF-TOKEN`).
fn play_authorize_get() -> Result<Request> {
	let cookie_str = settings::get_web_cookies();
	print(format!("[dlsite-play] → GET {}", PLAY_AUTHORIZE_URL));
	print(format!(
		"[dlsite-play]     User-Agent: {}",
		PLAY_AIOHTTP_USER_AGENT
	));
	print(format!("[dlsite-play]     Referer: {}", PLAY_REFERER));
	print(format!("[dlsite-play]     Accept: */*"));
	match cookie_str.as_deref() {
		Some(c) if !c.is_empty() => print(format!("[dlsite-play]     Cookie: {}", c)),
		_ => print(format!(
			"[dlsite-play]     Cookie: <none stored; complete web login first>"
		)),
	}
	let mut req = Request::get(PLAY_AUTHORIZE_URL)?
		.header("User-Agent", PLAY_AIOHTTP_USER_AGENT)
		.header("Referer", PLAY_REFERER)
		.header("Accept", "*/*");
	if let Some(ref c) = cookie_str {
		req = req.header("Cookie", c.as_str());
	}
	Ok(req)
}

fn play_api_get_with_cookie(url: &str, cookie_override: Option<&str>) -> Result<Request> {
	let cookie_str = cookie_override.map(String::from).or_else(settings::get_web_cookies);
	let accept = accept_for_url(url);
	let referer = url.contains("play.dl.dlsite.com").then_some(PLAY_REFERER);
	print(format!("[dlsite-play] → GET {}", url));
	print(format!(
		"[dlsite-play]     User-Agent: {}",
		PLAY_AIOHTTP_USER_AGENT
	));
	print(format!("[dlsite-play]     Accept: {}", accept));
	if let Some(r) = referer {
		print(format!("[dlsite-play]     Referer: {}", r));
	}
	match cookie_str.as_deref() {
		Some(c) if !c.is_empty() => print(format!("[dlsite-play]     Cookie: {}", c)),
		_ => print(format!(
			"[dlsite-play]     Cookie: <none stored; complete web login first>"
		)),
	}
	let mut req = Request::get(url)?
		.header("User-Agent", PLAY_AIOHTTP_USER_AGENT)
		.header("Accept", accept);
	if let Some(r) = referer {
		req = req.header("Referer", r);
	}
	if let Some(ref c) = cookie_str {
		req = req.header("Cookie", c.as_str());
	}
	Ok(req)
}

/// JSON / API GET — same header model as dlsite-async aiohttp (no browser CORS simulation).
pub(crate) fn play_authenticated_get(url: &str) -> Result<Request> {
	play_api_get_with_cookie(url, None)
}

/// Optimized page images: browser UA + `Referer` (not aiohttp).
pub(crate) fn play_image_get(url: &str) -> Result<Request> {
	let cookie_str = settings::get_web_cookies();
	print(format!("[dlsite-play] → GET {} (image)", url));
	print(format!("[dlsite-play]     User-Agent: {}", PLAY_IMAGE_USER_AGENT));
	print(format!("[dlsite-play]     Referer: {}", PLAY_REFERER));
	match cookie_str.as_deref() {
		Some(c) if !c.is_empty() => print(format!("[dlsite-play]     Cookie: {}", c)),
		_ => print(format!(
			"[dlsite-play]     Cookie: <none stored; complete web login first>"
		)),
	}
	let mut req = Request::get(url)?
		.header("User-Agent", PLAY_IMAGE_USER_AGENT)
		.header("Referer", PLAY_REFERER)
		.header("Accept", "image/webp,image/apng,image/*,*/*;q=0.8");
	if let Some(ref c) = cookie_str {
		req = req.header("Cookie", c.as_str());
	}
	Ok(req)
}

fn play_post_json_with_cookie(url: &str, body: &[u8], cookie: Option<&str>) -> Result<Request> {
	let cookie_str = cookie.map(String::from).or_else(settings::get_web_cookies);
	let xsrf = cookie_str.as_deref().and_then(xsrf_token_for_header);
	let referer = url.contains("play.dl.dlsite.com").then_some(PLAY_REFERER);
	print(format!("[dlsite-play] → POST {}", url));
	print(format!(
		"[dlsite-play]     User-Agent: {}",
		PLAY_AIOHTTP_USER_AGENT
	));
	print(format!("[dlsite-play]     Accept: application/json"));
	print(format!("[dlsite-play]     Content-Type: application/json"));
	if let Some(r) = referer {
		print(format!("[dlsite-play]     Referer: {}", r));
	}
	match xsrf {
		Some(ref x) if !x.is_empty() => print(format!("[dlsite-play]     X-XSRF-TOKEN: {}", x)),
		_ => print(format!("[dlsite-play]     X-XSRF-TOKEN: <missing>")),
	}
	match cookie_str.as_deref() {
		Some(c) if !c.is_empty() => print(format!("[dlsite-play]     Cookie: {}", c)),
		_ => print(format!(
			"[dlsite-play]     Cookie: <none stored; complete web login first>"
		)),
	}
	print(format!("[dlsite-play]     body: {} bytes", body.len()));
	let mut req = Request::post(url)?
		.header("User-Agent", PLAY_AIOHTTP_USER_AGENT)
		.header("Accept", "application/json")
		.header("Content-Type", "application/json");
	if let Some(r) = referer {
		req = req.header("Referer", r);
	}
	if let Some(ref x) = xsrf {
		req = req.header("X-XSRF-TOKEN", x.as_str());
	}
	if let Some(ref c) = cookie_str {
		req = req.header("Cookie", c.as_str());
	}
	Ok(req.body(body))
}

fn body_preview(data: &[u8]) -> String {
	match str::from_utf8(data) {
		Ok(s) => {
			let mut it = s.chars();
			let chunk: String = it.by_ref().take(280).collect();
			if it.next().is_some() {
				chunk + "..."
			} else {
				chunk
			}
		}
		Err(_) => format!("<non-utf8 {} bytes>", data.len()),
	}
}

fn log_http_failure(op: &str, status: i32, data: &[u8]) {
	print(format!(
		"[dlsite-play] {} HTTP {} ({} bytes) {}",
		op,
		status,
		data.len(),
		body_preview(data)
	));
}

fn ensure_ok(op: &str, status: i32, data: &[u8]) -> Result<()> {
	if (200..300).contains(&status) {
		return Ok(());
	}
	log_http_failure(op, status, data);
	if status == 401 {
		bail!("{} HTTP 401: session expired, complete web login again.", op);
	}
	bail!("{} HTTP {}: {}", op, status, body_preview(data));
}

/// Fetch the list of purchased work IDs (sorted by sales date, newest first).
pub fn get_sales() -> Result<Vec<SalesEntry>> {
	print(format!(
		"[dlsite-play] get_sales (build v46; aiohttp-style API + image UA split; 401 retry)"
	));
	if settings::get_web_cookies().is_some() {
		prime_play_api_session()?;
	}
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let mut resp = play_authenticated_get(url.as_str())?.send()?;
	let mut status = resp.status_code();
	let mut data = resp.get_data()?;
	if status == 401 && settings::get_web_cookies().is_some() {
		print(format!(
			"[dlsite-play] get_sales HTTP 401, running prime again and retrying once"
		));
		prime_play_api_session()?;
		resp = play_authenticated_get(url.as_str())?.send()?;
		status = resp.status_code();
		data = resp.get_data()?;
	}
	ensure_ok("get_sales", status, &data)?;
	let entries: Vec<SalesEntry> = serde_json::from_slice(&data).map_err(|e| {
		print(format!(
			"[dlsite-play] get_sales parse error: {} status={} {} bytes preview: {}",
			e,
			status,
			data.len(),
			body_preview(&data)
		));
		error!(
			"Failed to parse sales response: {} ({} bytes). Body: {}",
			e,
			data.len(),
			body_preview(&data)
		)
	})?;
	Ok(entries)
}

/// Fetch full work metadata for a batch of work IDs.
/// The Play API accepts up to 100 work IDs per request.
pub fn get_works(worknos: &[String]) -> Result<Vec<PurchaseWork>> {
	let mut all_works: Vec<PurchaseWork> = Vec::new();

	for (chunk_idx, chunk) in worknos.chunks(100).enumerate() {
		let url = format!("{}/content/works", PLAY_API);
		let body = serde_json::to_vec(chunk).map_err(|_| error!("Failed to serialize work IDs"))?;
		let resp = play_post_json_with_cookie(url.as_str(), &body, None)?.send()?;
		let status = resp.status_code();
		let data = resp.get_data()?;
		let op = format!("get_works chunk {}", chunk_idx);
		ensure_ok(&op, status, &data)?;
		let parsed: WorksResponse = serde_json::from_slice(&data).map_err(|e| {
			print(format!(
				"[dlsite-play] get_works parse error chunk={} {} status={} {} bytes preview: {}",
				chunk_idx,
				e,
				status,
				data.len(),
				body_preview(&data)
			));
			error!(
				"Failed to parse works response: {} ({} bytes). Body: {}",
				e,
				data.len(),
				body_preview(&data)
			)
		})?;
		all_works.extend(parsed.works);
	}

	Ok(all_works)
}

/// Get a download token for a specific work.
pub fn download_token(workno: &str) -> Result<DownloadToken> {
	let url = format!("{}/download/sign/cookie?workno={}", PLAY_DL_API, workno);
	let resp = play_authenticated_get(url.as_str())?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	ensure_ok("download_token", status, &data)?;
	let token: DownloadToken = serde_json::from_slice(&data).map_err(|e| {
		print(format!(
			"[dlsite-play] download_token parse error workno={} {} status={} preview: {}",
			workno,
			e,
			status,
			body_preview(&data)
		));
		error!(
			"Failed to parse download token: {} ({} bytes). Body: {}",
			e,
			data.len(),
			body_preview(&data)
		)
	})?;
	Ok(token)
}

/// Fetch the ziptree for a download token.
pub fn fetch_ziptree(token: &DownloadToken) -> Result<ZipTree> {
	let url = format!("{}ziptree.json", token.url);
	let resp = play_authenticated_get(url.as_str())?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	ensure_ok("fetch_ziptree", status, &data)?;
	let raw: RawZipTree = serde_json::from_slice(&data).map_err(|e| {
		print(format!(
			"[dlsite-play] fetch_ziptree parse error {} status={} preview: {}",
			e,
			status,
			body_preview(&data)
		));
		error!(
			"Failed to parse ziptree: {} ({} bytes). Body: {}",
			e,
			data.len(),
			body_preview(&data)
		)
	})?;
	Ok(ZipTree::from_raw(raw))
}

/// Build the URL for downloading an optimized file.
pub fn optimized_url(token: &DownloadToken, optimized_name: &str) -> String {
	format!("{}optimized/{}", token.url, optimized_name)
}
