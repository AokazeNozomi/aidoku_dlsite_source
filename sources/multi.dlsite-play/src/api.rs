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

const PLAY_API: &str = "https://play.dlsite.com/api/v3";
const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";

/// Log outgoing request headers. `cookie` is the value we send (from web login), if any.
pub(crate) fn log_outgoing_request(
	method: &str,
	url: &str,
	headers: &[(&str, &str)],
	body_len: Option<usize>,
	cookie: Option<&str>,
) {
	print(format!("[dlsite-play] → {} {}", method, url));
	for (name, value) in headers {
		print(format!("[dlsite-play]     {}: {}", name, value));
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

fn play_get(url: &str) -> Result<Request> {
	let cookie = settings::get_web_cookies();
	log_outgoing_request(
		"GET",
		url,
		&[("Referer", PLAY_REFERER)],
		None,
		cookie.as_deref(),
	);
	let mut req = Request::get(url)?.header("Referer", PLAY_REFERER);
	if let Some(ref c) = cookie {
		req = req.header("Cookie", c.as_str());
	}
	Ok(req)
}

fn play_post_json(url: &str, body: &[u8]) -> Result<Request> {
	let cookie = settings::get_web_cookies();
	log_outgoing_request(
		"POST",
		url,
		&[
			("Referer", PLAY_REFERER),
			("Content-Type", "application/json"),
		],
		Some(body.len()),
		cookie.as_deref(),
	);
	let mut req = Request::post(url)?
		.header("Referer", PLAY_REFERER)
		.header("Content-Type", "application/json");
	if let Some(ref c) = cookie {
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

/// Fetch the list of purchased work IDs (sorted by sales date, newest first).
pub fn get_sales() -> Result<Vec<SalesEntry>> {
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let resp = play_get(&url)?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		log_http_failure("get_sales", status, &data);
	}
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
		let resp = play_post_json(&url, &body)?.send()?;
		let status = resp.status_code();
		let data = resp.get_data()?;
		if !(200..300).contains(&status) {
			log_http_failure(&format!("get_works chunk {}", chunk_idx), status, &data);
		}
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
	let resp = play_get(&url)?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		log_http_failure("download_token", status, &data);
	}
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
	let resp = play_get(&url)?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		log_http_failure("fetch_ziptree", status, &data);
	}
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
