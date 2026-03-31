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

const PLAY_API: &str = "https://play.dlsite.com/api/v3";
const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";
/// Establishes Play API session after DLsite login (see `dlsite-async` `PlayAPI.login`).
const PLAY_AUTHORIZE_URL: &str = "https://play.dlsite.com/api/authorize";

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

/// Merge `Set-Cookie` (first `name=value` of each line) into the stored `Cookie` header value.
fn apply_set_cookie_headers(existing: Option<&str>, set_cookie: Option<String>) -> Option<String> {
	let raw = set_cookie.filter(|s| !s.is_empty())?;
	let mut pairs: Vec<(String, String)> = Vec::new();
	if let Some(ex) = existing {
		for part in ex.split(';') {
			let p = part.trim();
			if let Some((n, v)) = p.split_once('=') {
				pairs.push((n.trim().into(), v.trim().into()));
			}
		}
	}
	for block in raw.split('\n') {
		let line = block.trim();
		if line.is_empty() {
			continue;
		}
		let first = line.split(';').next().unwrap_or("").trim();
		let Some((n, v)) = first.split_once('=') else {
			continue;
		};
		let name = n.trim();
		let value = v.trim();
		if let Some(idx) = pairs
			.iter()
			.position(|(k, _)| k.eq_ignore_ascii_case(name))
		{
			pairs[idx] = (name.into(), value.into());
		} else {
			pairs.push((name.into(), value.into()));
		}
	}
	if pairs.is_empty() {
		return existing.map(Into::into);
	}
	pairs.sort_by(|a, b| a.0.cmp(&b.0));
	let merged = pairs
		.into_iter()
		.map(|(n, v)| format!("{}={}", n, v))
		.collect::<Vec<_>>()
		.join("; ");
	Some(merged)
}

/// `GET /api/authorize` ties DLsite account cookies to Play’s Laravel session (dlsite-async does this after login).
pub(crate) fn prime_play_api_session() -> Result<()> {
	if settings::get_web_cookies().is_none() {
		return Ok(());
	}
	let resp = play_authenticated_get(PLAY_AUTHORIZE_URL)?.send()?;
	let status = resp.status_code();
	let set_cookie = resp
		.get_header("Set-Cookie")
		.or_else(|| resp.get_header("set-cookie"));
	let _ = resp.get_data();
	if !(200..300).contains(&status) {
		print(format!(
			"[dlsite-play] prime_play_api_session authorize HTTP {}",
			status
		));
		if status == 401 {
			bail!("authorize HTTP 401: session expired, complete web login again.");
		}
		return Ok(());
	}
	if let Some(merged) = apply_set_cookie_headers(settings::get_web_cookies().as_deref(), set_cookie) {
		settings::set_web_cookies(&merged);
		print(format!(
			"[dlsite-play] prime_play_api_session updated cookie jar ({} chars)",
			merged.len()
		));
	}
	Ok(())
}

fn play_authenticated_get_with_cookie(url: &str, cookie_override: Option<&str>) -> Result<Request> {
	let cookie_str = cookie_override.map(String::from).or_else(settings::get_web_cookies);
	let xsrf = cookie_str.as_deref().and_then(xsrf_token_for_header);
	let accept = accept_for_url(url);
	log_outgoing_request(
		"GET",
		url,
		&[
			("Referer", PLAY_REFERER),
			("Origin", PLAY_ORIGIN),
			("Accept", accept),
			("X-Requested-With", "XMLHttpRequest"),
		],
		None,
		cookie_str.as_deref(),
		xsrf.as_deref(),
	);
	let mut req = Request::get(url)?
		.header("Referer", PLAY_REFERER)
		.header("Origin", PLAY_ORIGIN)
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
	if status == 401 {
		bail!("{} HTTP 401: session expired, complete web login again.", op);
	}
	bail!("{} HTTP {}: {}", op, status, body_preview(data));
}

/// Fetch the list of purchased work IDs (sorted by sales date, newest first).
pub fn get_sales() -> Result<Vec<SalesEntry>> {
	prime_play_api_session()?;
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let resp = play_authenticated_get(url.as_str())?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
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
