use crate::settings;
use aidoku::{
	alloc::{format, String, Vec},
	imports::net::Request,
	imports::std::print,
	prelude::*,
	Result,
};
use core::str;

pub(super) const PLAY_REFERER: &str = "https://play.dlsite.com/";
/// dlsite-async `PlayAPI` uses plain `aiohttp.ClientSession` — no browser `Origin` / `Sec-Fetch-*` / `X-Requested-With`.
const PLAY_AIOHTTP_USER_AGENT: &str = "Python/3.12 aiohttp/3.11.16";
/// CDN / optimized assets: keep a real browser UA so hotlink rules stay happy.
const PLAY_IMAGE_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";

pub(super) const PLAY_API: &str = "https://play.dlsite.com/api/v3";
pub(super) const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";
pub(super) const PLAY_BASE: &str = "https://play.dlsite.com";

pub(super) fn hex_digit(b: u8) -> Option<u8> {
	match b {
		b'0'..=b'9' => Some(b - b'0'),
		b'a'..=b'f' => Some(10 + b - b'a'),
		b'A'..=b'F' => Some(10 + b - b'A'),
		_ => None,
	}
}

/// Browser stacks send `X-XSRF-TOKEN` as URL-decoded cookie value (see Laravel / axios).
pub(super) fn percent_decode_cookie_value(input: &str) -> String {
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

pub(super) fn xsrf_token_for_header(cookie_header: &str) -> Option<String> {
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
	if url.contains("/api/") {
		return "application/json";
	}
	"*/*"
}

/// Build a GET request.  **No manual `Cookie` header** — Aidoku's runtime
/// injects cookies from `HTTPCookieStorage` (synced from the WebView's
/// `WKWebsiteDataStore.default()`).  Setting `Cookie` here would create
/// duplicates that confuse the server.
fn play_api_get(url: &str) -> Result<Request> {
	let accept = accept_for_url(url);
	let referer = url.contains("play.dl.dlsite.com").then_some(PLAY_REFERER);
	print(format!("[dlsite-play] → GET {}", url));
	let mut req = Request::get(url)?
		.header("User-Agent", PLAY_AIOHTTP_USER_AGENT)
		.header("Accept", accept);
	if let Some(r) = referer {
		req = req.header("Referer", r);
	}
	Ok(req)
}

/// JSON / API GET.
pub(super) fn play_authenticated_get(url: &str) -> Result<Request> {
	play_api_get(url)
}

/// Optimized page images: browser UA + `Referer` (not aiohttp).
pub(crate) fn play_image_get(url: &str) -> Result<Request> {
	print(format!("[dlsite-play] → GET {} (image)", url));
	let req = Request::get(url)?
		.header("User-Agent", PLAY_IMAGE_USER_AGENT)
		.header("Referer", PLAY_REFERER)
		.header("Accept", "image/webp,image/apng,image/*,*/*;q=0.8");
	Ok(req)
}

/// Build a POST request.  `X-XSRF-TOKEN` header is read from the stored
/// cookie snapshot (needed for Laravel CSRF on state-changing requests).
/// Cookie header is left to Aidoku's runtime.
pub(super) fn play_post_json(url: &str, body: &[u8]) -> Result<Request> {
	let xsrf = settings::get_web_cookies()
		.as_deref()
		.and_then(xsrf_token_for_header);
	let referer = url.contains("play.dl.dlsite.com").then_some(PLAY_REFERER);
	print(format!("[dlsite-play] → POST {}", url));
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
	Ok(req.body(body))
}

pub(super) fn body_preview(data: &[u8]) -> String {
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

pub(super) fn ensure_ok(op: &str, status: i32, data: &[u8]) -> Result<()> {
	if (200..300).contains(&status) {
		return Ok(());
	}
	log_http_failure(op, status, data);
	if status == 401 {
		bail!("{} HTTP 401: session expired, complete web login again.", op);
	}
	bail!("{} HTTP {}: {}", op, status, body_preview(data));
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

	// -- hex_digit tests --

	#[aidoku_test]
	fn hex_digit_digits() {
		assert_eq!(hex_digit(b'0'), Some(0));
		assert_eq!(hex_digit(b'5'), Some(5));
		assert_eq!(hex_digit(b'9'), Some(9));
	}

	#[aidoku_test]
	fn hex_digit_lowercase() {
		assert_eq!(hex_digit(b'a'), Some(10));
		assert_eq!(hex_digit(b'f'), Some(15));
	}

	#[aidoku_test]
	fn hex_digit_uppercase() {
		assert_eq!(hex_digit(b'A'), Some(10));
		assert_eq!(hex_digit(b'F'), Some(15));
	}

	#[aidoku_test]
	fn hex_digit_invalid() {
		assert_eq!(hex_digit(b'g'), None);
		assert_eq!(hex_digit(b'G'), None);
		assert_eq!(hex_digit(b' '), None);
		assert_eq!(hex_digit(b'%'), None);
	}

	// -- percent_decode_cookie_value tests --

	#[aidoku_test]
	fn percent_decode_plain_string() {
		assert_eq!(percent_decode_cookie_value("hello"), "hello");
	}

	#[aidoku_test]
	fn percent_decode_encoded_chars() {
		assert_eq!(percent_decode_cookie_value("hello%20world"), "hello world");
		assert_eq!(percent_decode_cookie_value("%2F"), "/");
		assert_eq!(percent_decode_cookie_value("%3D"), "=");
	}

	#[aidoku_test]
	fn percent_decode_multiple_encoded() {
		assert_eq!(
			percent_decode_cookie_value("a%20b%20c"),
			"a b c"
		);
	}

	#[aidoku_test]
	fn percent_decode_incomplete_sequence() {
		// Incomplete percent sequence should be passed through
		assert_eq!(percent_decode_cookie_value("abc%2"), "abc%2");
		assert_eq!(percent_decode_cookie_value("abc%"), "abc%");
	}

	#[aidoku_test]
	fn percent_decode_invalid_hex() {
		// Invalid hex chars after % should be passed through
		assert_eq!(percent_decode_cookie_value("%ZZ"), "%ZZ");
	}

	// -- xsrf_token_for_header tests --

	#[aidoku_test]
	fn xsrf_token_found() {
		let header = "XSRF-TOKEN=abc123; play_session=xyz";
		assert_eq!(xsrf_token_for_header(header), Some("abc123".into()));
	}

	#[aidoku_test]
	fn xsrf_token_encoded() {
		let header = "XSRF-TOKEN=abc%3D123; other=val";
		assert_eq!(xsrf_token_for_header(header), Some("abc=123".into()));
	}

	#[aidoku_test]
	fn xsrf_token_missing() {
		let header = "play_session=xyz; other=abc";
		assert_eq!(xsrf_token_for_header(header), None);
	}

	#[aidoku_test]
	fn xsrf_token_case_insensitive() {
		let header = "xsrf-token=mytoken; other=val";
		assert_eq!(xsrf_token_for_header(header), Some("mytoken".into()));
	}

	// -- accept_for_url tests --

	#[aidoku_test]
	fn accept_for_api_v3_url() {
		assert_eq!(accept_for_url("https://play.dlsite.com/api/v3/content/sales"), "application/json");
	}

	#[aidoku_test]
	fn accept_for_api_url() {
		assert_eq!(accept_for_url("https://play.dlsite.com/api/authorize"), "application/json");
	}

	#[aidoku_test]
	fn accept_for_non_api_url() {
		assert_eq!(accept_for_url("https://play.dlsite.com/work/RJ123/view"), "*/*");
	}
}
