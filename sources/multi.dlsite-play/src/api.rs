use crate::models::{DownloadToken, PurchaseWork, RawZipTree, SalesEntry, WorksResponse, ZipTree};
use aidoku::{
	alloc::{format, String, Vec},
	imports::net::Request,
	prelude::*,
	Result,
};

const PLAY_API: &str = "https://play.dlsite.com/api/v3";
const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";
const REFERER: &str = "https://play.dlsite.com/";

fn play_get(url: &str) -> Result<Request> {
	Ok(Request::get(url)?.header("Referer", REFERER))
}

fn play_post(url: &str) -> Result<Request> {
	Ok(Request::post(url)?
		.header("Referer", REFERER)
		.header("Content-Type", "application/json"))
}

/// Fetch the list of purchased work IDs (sorted by sales date, newest first).
pub fn get_sales() -> Result<Vec<SalesEntry>> {
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let data = play_get(&url)?.send()?.get_data()?;
	let entries: Vec<SalesEntry> =
		serde_json::from_slice(&data).map_err(|_| error!("Failed to parse sales response"))?;
	Ok(entries)
}

/// Fetch full work metadata for a batch of work IDs.
/// The Play API accepts up to 100 work IDs per request.
pub fn get_works(worknos: &[String]) -> Result<Vec<PurchaseWork>> {
	let mut all_works: Vec<PurchaseWork> = Vec::new();

	for chunk in worknos.chunks(100) {
		let url = format!("{}/content/works", PLAY_API);
		let body = serde_json::to_vec(chunk).map_err(|_| error!("Failed to serialize work IDs"))?;
		let data = play_post(&url)?.body(&body).send()?.get_data()?;
		let resp: WorksResponse =
			serde_json::from_slice(&data).map_err(|_| error!("Failed to parse works response"))?;
		all_works.extend(resp.works);
	}

	Ok(all_works)
}

/// Get a download token for a specific work.
pub fn download_token(workno: &str) -> Result<DownloadToken> {
	let url = format!("{}/download/sign/cookie?workno={}", PLAY_DL_API, workno);
	let data = play_get(&url)?.send()?.get_data()?;
	let token: DownloadToken =
		serde_json::from_slice(&data).map_err(|_| error!("Failed to parse download token"))?;
	Ok(token)
}

/// Fetch the ziptree for a download token.
pub fn fetch_ziptree(token: &DownloadToken) -> Result<ZipTree> {
	let url = format!("{}ziptree.json", token.url);
	let data = play_get(&url)?.send()?.get_data()?;
	let raw: RawZipTree =
		serde_json::from_slice(&data).map_err(|_| error!("Failed to parse ziptree"))?;
	Ok(ZipTree::from_raw(raw))
}

/// Build the URL for downloading an optimized file.
pub fn optimized_url(token: &DownloadToken, optimized_name: &str) -> String {
	format!("{}optimized/{}", token.url, optimized_name)
}
