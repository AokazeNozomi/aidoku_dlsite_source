use crate::models::{LanguageEdition, ProductInfo};
use aidoku::{
	alloc::{format, Vec},
	imports::{net::Request, std::print},
	Result,
};

pub use dlsite_common::api::get_public_work_details;

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
