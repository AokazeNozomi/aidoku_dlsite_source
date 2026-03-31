use crate::models::{DownloadToken, PurchaseWork, RawZipTree, SalesEntry, WorksResponse, ZipTree};
use crate::settings;
use aidoku::{
	alloc::{format, String, Vec},
	imports::{net::Request, std::print},
	prelude::*,
	Result,
};
use core::str;

pub(crate) const PLAY_REFERER: &str = "https://play.dlsite.com/";
const PLAY_ORIGIN: &str = "https://play.dlsite.com";
const LOGIN_ORIGIN: &str = "https://login.dlsite.com";

const PLAY_API: &str = "https://play.dlsite.com/api/v3";
const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";
const LOGIN_URL: &str = "https://login.dlsite.com/login";
const PLAY_LOGIN_URL: &str = "https://play.dlsite.com/login/";
const PLAY_AUTHORIZE_URL: &str = "https://play.dlsite.com/api/authorize";
const LOGIN_HOST: &str = "login.dlsite.com";
const PLAY_HOST: &str = "play.dlsite.com";

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

fn status_is_ok_or_redirect(status: i32) -> bool {
	(200..400).contains(&status)
}

fn percent_encode_form_value(input: &str) -> String {
	let bytes = input.as_bytes();
	let mut out = String::new();
	for &b in bytes {
		match b {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
				out.push(char::from(b))
			}
			b' ' => out.push('+'),
			_ => {
				out.push('%');
				let hi = b >> 4;
				let lo = b & 0x0F;
				out.push(char::from(if hi < 10 { b'0' + hi } else { b'A' + (hi - 10) }));
				out.push(char::from(if lo < 10 { b'0' + lo } else { b'A' + (lo - 10) }));
			}
		}
	}
	out
}

fn extract_attr_value(tag: &str, attr: &str) -> Option<String> {
	let needle = format!("{}=\"", attr);
	let start = tag.find(needle.as_str())? + needle.len();
	let rest = tag.get(start..)?;
	let end = rest.find('"')?;
	rest.get(..end).map(String::from)
}

fn parse_login_token(html: &str) -> Option<String> {
	for segment in html.split("<input") {
		if !segment.contains("name=\"_token\"") {
			continue;
		}
		let mut tag = String::from("<input");
		tag.push_str(segment);
		return extract_attr_value(tag.as_str(), "value");
	}
	None
}

#[derive(Clone, Debug)]
struct CookieEntry {
	name: String,
	value: String,
	domain: String,
	path: String,
}

fn normalize_domain(input: &str) -> String {
	String::from(
		input
		.trim()
		.trim_start_matches('.')
		.to_ascii_lowercase(),
	)
}

fn upsert_cookie(cookies: &mut Vec<CookieEntry>, cookie: CookieEntry) {
	for existing in cookies.iter_mut() {
		if existing.name.as_str().eq_ignore_ascii_case(cookie.name.as_str())
			&& existing.domain.as_str().eq_ignore_ascii_case(cookie.domain.as_str())
			&& existing.path == cookie.path
		{
			existing.value = cookie.value;
			return;
		}
	}
	cookies.push(cookie);
}

fn split_set_cookie_header(header: &str) -> Vec<String> {
	let bytes = header.as_bytes();
	let mut parts: Vec<String> = Vec::new();
	let mut start = 0usize;
	let mut i = 0usize;
	while i < bytes.len() {
		if bytes[i] == b',' {
			let mut lookahead = i + 1;
			while lookahead < bytes.len() && bytes[lookahead] == b' ' {
				lookahead += 1;
			}
			let mut bound = lookahead;
			while bound < bytes.len() && bytes[bound] != b';' && bytes[bound] != b',' {
				bound += 1;
			}
			if let Some(candidate) = header.get(lookahead..bound) {
				if candidate.contains('=') {
					if let Some(chunk) = header.get(start..i) {
						let trimmed = chunk.trim();
						if !trimmed.is_empty() {
							parts.push(String::from(trimmed));
						}
					}
					start = i + 1;
				}
			}
		}
		i += 1;
	}
	if let Some(chunk) = header.get(start..) {
		let trimmed = chunk.trim();
		if !trimmed.is_empty() {
			parts.push(String::from(trimmed));
		}
	}
	parts
}

fn parse_set_cookie_header(header: &str, default_domain: &str, cookies: &mut Vec<CookieEntry>) {
	for part in split_set_cookie_header(header) {
		let mut attrs = part.split(';').map(|s| s.trim());
		let first = attrs.next().unwrap_or_default();
		let Some((name, value)) = first.split_once('=') else {
			continue;
		};
		let name = name.trim();
		let value = value.trim();
		if name.is_empty() || value.is_empty() {
			continue;
		}
		let mut domain = String::from(default_domain);
		let mut path = String::from("/");
		for attr in attrs {
			let Some((k, v)) = attr.split_once('=') else {
				continue;
			};
			if k.eq_ignore_ascii_case("Domain") {
				domain = normalize_domain(v);
			} else if k.eq_ignore_ascii_case("Path") {
				let trimmed = v.trim();
				if !trimmed.is_empty() {
					path = String::from(trimmed);
				}
			}
		}
		upsert_cookie(
			cookies,
			CookieEntry {
				name: String::from(name),
				value: String::from(value),
				domain,
				path,
			},
		);
	}
}

fn ingest_response_cookies(
	resp: &aidoku::imports::net::Response,
	default_domain: &str,
	cookies: &mut Vec<CookieEntry>,
) {
	if let Some(set_cookie) = resp.get_header("Set-Cookie") {
		parse_set_cookie_header(set_cookie.as_str(), default_domain, cookies);
	}
	if let Some(set_cookie) = resp.get_header("set-cookie") {
		parse_set_cookie_header(set_cookie.as_str(), default_domain, cookies);
	}
}

fn domain_matches(host: &str, cookie_domain: &str) -> bool {
	let host = host.to_ascii_lowercase();
	let cookie_domain = cookie_domain.to_ascii_lowercase();
	host == cookie_domain || host.ends_with(format!(".{}", cookie_domain).as_str())
}

fn path_matches(request_path: &str, cookie_path: &str) -> bool {
	let normalized = if cookie_path.is_empty() { "/" } else { cookie_path };
	request_path.starts_with(normalized)
}

fn build_cookie_header_for_host(cookies: &[CookieEntry], host: &str, request_path: &str) -> String {
	let mut pairs: Vec<(String, String)> = cookies
		.iter()
		.filter(|c| domain_matches(host, c.domain.as_str()) && path_matches(request_path, c.path.as_str()))
		.map(|c| (c.name.clone(), c.value.clone()))
		.collect();
	pairs.sort_by(|a, b| a.0.cmp(&b.0));
	pairs
		.into_iter()
		.map(|(k, v)| format!("{}={}", k, v))
		.collect::<Vec<String>>()
		.join("; ")
}

fn looks_like_login_page(html: &str) -> bool {
	html.contains("name=\"login_id\"")
		|| html.contains("id=\"form_id\"")
		|| html.contains("ログインID")
		|| html.contains("password")
}

fn looks_like_challenge_page(html: &str) -> bool {
	html.contains("recaptcha")
		|| html.contains("g-recaptcha")
		|| html.contains("captcha")
		|| html.contains("Cloudflare")
		|| html.contains("Access denied")
}

fn build_login_form_body(token: &str, username: &str, password: &str) -> String {
	format!(
		"_token={}&login_id={}&password={}",
		percent_encode_form_value(token),
		percent_encode_form_value(username),
		percent_encode_form_value(password),
	)
}

fn play_authenticated_get_with_cookie(url: &str, cookie: Option<&str>) -> Result<Request> {
	let cookie_str = cookie.map(String::from).or_else(settings::get_web_cookies);
	let xsrf = cookie_str.as_deref().and_then(xsrf_token_for_header);
	let accept = accept_for_url(url);
	log_outgoing_request(
		"GET",
		url,
		&[
			("Referer", PLAY_REFERER),
			("Accept", accept),
			("X-Requested-With", "XMLHttpRequest"),
		],
		None,
		cookie_str.as_deref(),
		xsrf.as_deref(),
	);
	let mut req = Request::get(url)?
		.header("Referer", PLAY_REFERER)
		.header("Accept", accept)
		.header("X-Requested-With", "XMLHttpRequest");
	if let Some(ref x) = xsrf {
		req = req.header("X-XSRF-TOKEN", x.as_str());
	}
	if let Some(ref c) = cookie_str {
		req = req.header("Cookie", c.as_str());
	}
	Ok(req)
}

fn accept_for_url(url: &str) -> &'static str {
	if url.contains("/api/") {
		"application/json"
	} else {
		"*/*"
	}
}

/// Log outgoing request. `xsrf` is decoded token for `X-XSRF-TOKEN` when present.
pub(crate) fn log_outgoing_request(
	method: &str,
	url: &str,
	headers: &[(&str, &str)],
	body_len: Option<usize>,
	cookie: Option<&str>,
	xsrf: Option<&str>,
) {
	print(format!("[dlsite-play] → {} {}", method, url));
	for (name, value) in headers {
		print(format!("[dlsite-play]     {}: {}", name, value));
	}
	match xsrf {
		Some(x) if !x.is_empty() => print(format!("[dlsite-play]     X-XSRF-TOKEN: {}", x)),
		_ => print(format!("[dlsite-play]     X-XSRF-TOKEN: <missing>")),
	}
	match cookie {
		Some(c) if !c.is_empty() => print(format!("[dlsite-play]     Cookie: {}", c)),
		_ => print(format!(
			"[dlsite-play]     Cookie: <none stored; complete web login first>"
		)),
	}
	if let Some(n) = body_len {
		print(format!("[dlsite-play]     body: {} bytes", n));
	}
}

/// GET with Referer, Accept, optional Cookie + `X-XSRF-TOKEN` (required by Play’s Laravel stack).
pub(crate) fn play_authenticated_get(url: &str) -> Result<Request> {
	play_authenticated_get_with_cookie(url, None)
}

fn play_post_json_with_cookie(url: &str, body: &[u8], cookie: Option<&str>) -> Result<Request> {
	let cookie_str = cookie.map(String::from).or_else(settings::get_web_cookies);
	let xsrf = cookie_str.as_deref().and_then(xsrf_token_for_header);
	log_outgoing_request(
		"POST",
		url,
		&[
			("Referer", PLAY_REFERER),
			("Origin", PLAY_ORIGIN),
			("Accept", "application/json"),
			("Content-Type", "application/json"),
			("X-Requested-With", "XMLHttpRequest"),
		],
		Some(body.len()),
		cookie_str.as_deref(),
		xsrf.as_deref(),
	);
	let mut req = Request::post(url)?
		.header("Referer", PLAY_REFERER)
		.header("Origin", PLAY_ORIGIN)
		.header("Accept", "application/json")
		.header("Content-Type", "application/json")
		.header("X-Requested-With", "XMLHttpRequest");
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
	bail!("{} HTTP {}: {}", op, status, body_preview(data));
}

fn send_authenticated_get(url: &str, cookie: Option<&str>) -> Result<(i32, Vec<u8>)> {
	let resp = play_authenticated_get_with_cookie(url, cookie)?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	Ok((status, data))
}

fn send_authenticated_post(url: &str, body: &[u8], cookie: Option<&str>) -> Result<(i32, Vec<u8>)> {
	let resp = play_post_json_with_cookie(url, body, cookie)?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	Ok((status, data))
}

fn with_reauth_retry<F>(op: &str, mut send: F) -> Result<(i32, Vec<u8>)>
where
	F: FnMut() -> Result<(i32, Vec<u8>)>,
{
	let first = send()?;
	if first.0 != 401 {
		return Ok(first);
	}

	print(format!(
		"[dlsite-play] {} received HTTP 401, attempting credential reauth",
		op
	));
	settings::set_logged_in(false);
	settings::clear_web_cookies();

	login_from_settings().map_err(|e| {
		error!(
			"{} HTTP 401 and automatic credential reauth failed: {:?}",
			op, e
		)
	})?;

	send()
}

fn probe_session_cookie(cookie_header: &str) -> Result<()> {
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let (status, data) = send_authenticated_get(url.as_str(), Some(cookie_header))?;
	ensure_ok("probe_session", status, &data)
}

fn login_from_settings() -> Result<()> {
	let (username, password) = settings::get_credentials().ok_or_else(|| {
		error!(
			"Missing DLsite credentials. Set username/password in source settings."
		)
	})?;
	login_with_credentials(username.as_str(), password.as_str())
}

pub fn login_with_credentials(username: &str, password: &str) -> Result<()> {
	if username.is_empty() || password.is_empty() {
		bail!("DLsite username and password are required.");
	}

	let mut cookies: Vec<CookieEntry> = Vec::new();

	let login_page_url = format!("{}?user=self", LOGIN_URL);
	let login_page = Request::get(login_page_url.as_str())?
		.header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
		.send()?;
	ingest_response_cookies(&login_page, LOGIN_HOST, &mut cookies);
	let login_page_status = login_page.status_code();
	let login_page_data = login_page.get_data()?;
	ensure_ok("login_page", login_page_status, &login_page_data)?;
	let login_page_html = str::from_utf8(&login_page_data)
		.map_err(|_| error!("Failed to decode DLsite login page as utf-8"))?;
	let token = parse_login_token(login_page_html)
		.ok_or_else(|| error!("Failed to find DLsite login form token"))?;
	let login_cookie = build_cookie_header_for_host(&cookies, LOGIN_HOST, "/login");

	let form_body = build_login_form_body(token.as_str(), username, password);
	let mut login_req = Request::post(LOGIN_URL)?
		.header("Origin", LOGIN_ORIGIN)
		.header("Referer", login_page_url.as_str())
		.header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
		.header("Content-Type", "application/x-www-form-urlencoded")
		.body(form_body.as_bytes());
	if !login_cookie.is_empty() {
		login_req = login_req.header("Cookie", login_cookie.as_str());
	}
	let login_response = login_req.send()?;
	ingest_response_cookies(&login_response, LOGIN_HOST, &mut cookies);
	let login_status = login_response.status_code();
	let login_data = login_response.get_data()?;
	if !status_is_ok_or_redirect(login_status) {
		log_http_failure("login_submit", login_status, &login_data);
		bail!(
			"DLsite credential login failed (HTTP {}). Verify username/password.",
			login_status
		);
	}
	let login_text = str::from_utf8(&login_data).unwrap_or_default();
	if !login_text.contains("ログイン中です") {
		if looks_like_challenge_page(login_text) {
			bail!(
				"DLsite blocked automatic credential login with a challenge (captcha/verification)."
			);
		}
		if looks_like_login_page(login_text) {
			bail!("DLsite credential login failed. Check login ID/password.");
		}
		bail!(
			"DLsite login did not return the expected success page. Body: {}",
			body_preview(&login_data)
		);
	}

	let play_cookie = build_cookie_header_for_host(&cookies, PLAY_HOST, "/login");
	let mut play_login_req = Request::get(PLAY_LOGIN_URL)?
		.header("Referer", LOGIN_URL)
		.header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8");
	if !play_cookie.is_empty() {
		play_login_req = play_login_req.header("Cookie", play_cookie.as_str());
	}
	let play_login = play_login_req.send()?;
	ingest_response_cookies(&play_login, PLAY_HOST, &mut cookies);
	let play_login_status = play_login.status_code();
	let play_login_data = play_login.get_data()?;
	if !status_is_ok_or_redirect(play_login_status) {
		log_http_failure("play_login", play_login_status, &play_login_data);
		bail!("Failed to establish DLsite Play session (HTTP {}).", play_login_status);
	}

	let authorize_cookie = build_cookie_header_for_host(&cookies, PLAY_HOST, "/api/authorize");
	let mut authorize_req = Request::get(PLAY_AUTHORIZE_URL)?
		.header("Referer", PLAY_REFERER)
		.header("Accept", "application/json");
	if !authorize_cookie.is_empty() {
		authorize_req = authorize_req.header("Cookie", authorize_cookie.as_str());
	}
	let authorize = authorize_req.send()?;
	ingest_response_cookies(&authorize, PLAY_HOST, &mut cookies);
	let authorize_status = authorize.status_code();
	let authorize_data = authorize.get_data()?;
	if !status_is_ok_or_redirect(authorize_status) {
		log_http_failure("play_authorize", authorize_status, &authorize_data);
		bail!("DLsite Play authorize failed (HTTP {}).", authorize_status);
	}

	let cookie_header = build_cookie_header_for_host(&cookies, PLAY_HOST, "/");
	if !cookie_header.contains("play_session=") {
		bail!("DLsite login succeeded but no play_session cookie was returned.");
	}
	if !cookie_header.contains("XSRF-TOKEN=") {
		bail!("DLsite login succeeded but no XSRF-TOKEN cookie was returned.");
	}

	settings::set_web_cookies(cookie_header.as_str());
	probe_session_cookie(cookie_header.as_str())?;
	settings::set_logged_in(true);
	settings::clear_cached_worknos();
	settings::clear_cached_page();
	print(format!(
		"[dlsite-play] credential login stored Cookie header ({} chars)",
		cookie_header.len()
	));
	Ok(())
}

/// Fetch the list of purchased work IDs (sorted by sales date, newest first).
pub fn get_sales() -> Result<Vec<SalesEntry>> {
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let (status, data) = with_reauth_retry("get_sales", || {
		send_authenticated_get(url.as_str(), None)
	})?;
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
		let (status, data) = with_reauth_retry("get_works", || {
			send_authenticated_post(url.as_str(), &body, None)
		})?;
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
	let (status, data) = with_reauth_retry("download_token", || {
		send_authenticated_get(url.as_str(), None)
	})?;
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
	let (status, data) = with_reauth_retry("fetch_ziptree", || {
		send_authenticated_get(url.as_str(), None)
	})?;
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
