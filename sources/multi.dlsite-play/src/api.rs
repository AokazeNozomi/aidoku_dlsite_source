use crate::models::{
	DownloadToken, GenreInfo, GenresResponse, LanguageEdition, ProductInfo, PurchaseWork,
	RawZipTree, SalesEntry, WorksResponse, ZipTree,
};
use crate::settings;
use aidoku::{
	alloc::{format, String, Vec},
	imports::{net::Request, std::print},
	prelude::*,
	Result,
};
use core::str;

pub(crate) const PLAY_REFERER: &str = "https://play.dlsite.com/";
/// dlsite-async `PlayAPI` uses plain `aiohttp.ClientSession` — no browser `Origin` / `Sec-Fetch-*` / `X-Requested-With`.
const PLAY_AIOHTTP_USER_AGENT: &str = "Python/3.12 aiohttp/3.11.16";
/// CDN / optimized assets: keep a real browser UA so hotlink rules stay happy.
const PLAY_IMAGE_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";

const PLAY_API: &str = "https://play.dlsite.com/api/v3";
const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";

pub(crate) fn hex_digit(b: u8) -> Option<u8> {
	match b {
		b'0'..=b'9' => Some(b - b'0'),
		b'a'..=b'f' => Some(10 + b - b'a'),
		b'A'..=b'F' => Some(10 + b - b'A'),
		_ => None,
	}
}

/// Browser stacks send `X-XSRF-TOKEN` as URL-decoded cookie value (see Laravel / axios).
pub(crate) fn percent_decode_cookie_value(input: &str) -> String {
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

pub(crate) fn xsrf_token_for_header(cookie_header: &str) -> Option<String> {
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

pub(crate) fn accept_for_url(url: &str) -> &'static str {
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
pub(crate) fn play_authenticated_get(url: &str) -> Result<Request> {
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
fn play_post_json(url: &str, body: &[u8]) -> Result<Request> {
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
/// Returns the combined `WorksResponse` (works + series).
pub fn get_works(worknos: &[String]) -> Result<WorksResponse> {
	let mut all_works: Vec<PurchaseWork> = Vec::new();
	let mut all_series = Vec::new();

	for (chunk_idx, chunk) in worknos.chunks(100).enumerate() {
		let url = format!("{}/content/works", PLAY_API);
		let body = serde_json::to_vec(chunk).map_err(|_| error!("Failed to serialize work IDs"))?;
		let resp = play_post_json(url.as_str(), &body)?.send()?;
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
		all_series.extend(parsed.series);
	}

	Ok(WorksResponse {
		works: all_works,
		series: all_series,
	})
}

/// Resolve numeric genre IDs to localized names.
/// `POST /api/v3/genres` with body `{"genre_ids": [...]}`.
/// No authentication required.
pub fn get_genres(ids: &[u32]) -> Result<Vec<GenreInfo>> {
	let mut all_genres: Vec<GenreInfo> = Vec::new();

	for (chunk_idx, chunk) in ids.chunks(100).enumerate() {
		let url = format!("{}/genres", PLAY_API);
		let wrapper = serde_json::to_vec(&GenreRequest { genre_ids: chunk })
			.map_err(|_| error!("Failed to serialize genre IDs"))?;
		let resp = play_post_json(url.as_str(), &wrapper)?.send()?;
		let status = resp.status_code();
		let data = resp.get_data()?;
		let op = format!("get_genres chunk {}", chunk_idx);
		ensure_ok(&op, status, &data)?;
		let parsed: GenresResponse = serde_json::from_slice(&data).map_err(|e| {
			print(format!(
				"[dlsite-play] get_genres parse error chunk={} {} status={} preview: {}",
				chunk_idx,
				e,
				status,
				body_preview(&data)
			));
			error!(
				"Failed to parse genres response: {} ({} bytes)",
				e,
				data.len()
			)
		})?;
		all_genres.extend(parsed.genres);
	}

	Ok(all_genres)
}

/// Serialization wrapper for `POST /api/v3/genres` request body.
#[derive(serde::Serialize)]
struct GenreRequest<'a> {
	genre_ids: &'a [u32],
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

/// Fetch language editions from the public DLsite product API.
/// Returns an empty Vec on failure (non-fatal enrichment).
pub fn get_language_editions(workno: &str) -> Result<Vec<LanguageEdition>> {
	let url = format!(
		"https://www.dlsite.com/maniax/api/=/product.json?workno={}",
		workno
	);
	print(format!("[dlsite-play] → GET {} (public API)", url));
	let resp = Request::get(&url)?
		.header("Accept", "application/json")
		.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		print(format!(
			"[dlsite-play] public API HTTP {} for {}",
			status, workno
		));
		return Ok(Vec::new());
	}
	let products: Vec<ProductInfo> = serde_json::from_slice(&data).unwrap_or_default();
	Ok(products
		.into_iter()
		.next()
		.map(|p| p.language_editions)
		.unwrap_or_default())
}

/// Build the URL for downloading an optimized file.
pub fn optimized_url(token: &DownloadToken, optimized_name: &str) -> String {
	format!("{}optimized/{}", token.url, optimized_name)
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

	// -- optimized_url tests --

	#[aidoku_test]
	fn optimized_url_builds_correctly() {
		let token = DownloadToken {
			expires: "2025-01-01".into(),
			url: "https://cdn.example.com/dl/".into(),
		};
		assert_eq!(
			optimized_url(&token, "abc123.webp"),
			"https://cdn.example.com/dl/optimized/abc123.webp"
		);
	}
}
