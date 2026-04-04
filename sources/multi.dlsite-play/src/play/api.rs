use super::http::{
	body_preview, ensure_ok, play_authenticated_get, play_post_json, PLAY_API, PLAY_BASE,
	PLAY_DL_API,
};
use super::models::{DownloadToken, RawZipTree, ZipTree};
use crate::models::{
	GenreInfo, GenresResponse, PurchaseWork, SalesEntry, ViewHistoryEntry, WorksResponse,
};
use aidoku::{
	alloc::{format, String, Vec},
	prelude::*,
	Result,
};
use dlsite_common::debug_print;

/// Fetch the list of purchased work IDs (sorted by sales date, newest first).
pub fn get_sales() -> Result<Vec<SalesEntry>> {
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let resp = play_authenticated_get(url.as_str())?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	ensure_ok("get_sales", status, &data)?;
	let entries: Vec<SalesEntry> = serde_json::from_slice(&data).map_err(|e| {
		debug_print!(
			"[dlsite-play] get_sales parse error: {} status={} {} bytes preview: {}",
			e,
			status,
			data.len(),
			body_preview(&data)
		);
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
			debug_print!(
				"[dlsite-play] get_works parse error chunk={} {} status={} {} bytes preview: {}",
				chunk_idx,
				e,
				status,
				data.len(),
				body_preview(&data)
			);
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
			debug_print!(
				"[dlsite-play] get_genres parse error chunk={} {} status={} preview: {}",
				chunk_idx,
				e,
				status,
				body_preview(&data)
			);
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
		debug_print!(
			"[dlsite-play] download_token parse error workno={} {} status={} preview: {}",
			workno,
			e,
			status,
			body_preview(&data)
		);
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
		debug_print!(
			"[dlsite-play] fetch_ziptree parse error {} status={} preview: {}",
			e,
			status,
			body_preview(&data)
		);
		error!(
			"Failed to parse ziptree: {} ({} bytes). Body: {}",
			e,
			data.len(),
			body_preview(&data)
		)
	})?;
	Ok(ZipTree::from_raw(raw))
}

/// Fetch the user's recently viewed works with timestamps.
/// `GET /api/view_histories` → `[{workno, accessed_at}, ...]`
pub fn get_view_histories() -> Result<Vec<ViewHistoryEntry>> {
	let url = format!("{}/api/view_histories", PLAY_BASE);
	let resp = play_authenticated_get(url.as_str())?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	ensure_ok("get_view_histories", status, &data)?;
	let entries: Vec<ViewHistoryEntry> = serde_json::from_slice(&data).map_err(|e| {
		debug_print!(
			"[dlsite-play] get_view_histories parse error: {} status={} preview: {}",
			e,
			status,
			body_preview(&data)
		);
		error!(
			"Failed to parse view histories: {} ({} bytes)",
			e,
			data.len()
		)
	})?;
	Ok(entries)
}

/// Update the "recently opened" timestamp for a work.
/// `POST /api/view_histories` with `{"workno": "RJ..."}` → 204
pub fn post_view_history(workno: &str) -> Result<()> {
	let url = format!("{}/api/view_histories", PLAY_BASE);
	let body = format!("{{\"workno\":\"{}\"}}", workno);
	let resp = play_post_json(url.as_str(), body.as_bytes())?.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	ensure_ok("post_view_history", status, &data)?;
	Ok(())
}

/// Build the URL for downloading an optimized file.
pub fn optimized_url(token: &DownloadToken, optimized_name: &str) -> String {
	format!("{}optimized/{}", token.url, optimized_name)
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

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
