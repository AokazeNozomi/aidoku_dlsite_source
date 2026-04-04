use crate::models::{LanguageEdition, ProductInfo};
use aidoku::{
	alloc::{format, string::String, Vec},
	imports::{net::Request, std::print},
	Result,
};

pub use dlsite_common::api::get_public_work_details;

/// Result from the public product API containing language edition data.
pub struct LanguageResult {
	pub editions: Vec<LanguageEdition>,
	/// The queried work's own language code from `translation_info.lang`
	/// (e.g. `"CHI_HANS"`). `None` for originals.
	pub own_lang: Option<String>,
}

/// Fetch language editions from the public DLsite product API.
pub fn get_language_editions(workno: &str) -> Result<LanguageResult> {
	let url = format!(
		"https://www.dlsite.com/{}/api/=/product.json?workno={}",
		crate::DLSITE_SITE_SLUG, workno
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
		return Err(aidoku::AidokuError::message("public API non-2xx"));
	}
	let products: Vec<ProductInfo> = serde_json::from_slice(&data).unwrap_or_default();
	let product = products.into_iter().next();
	Ok(LanguageResult {
		editions: product
			.as_ref()
			.map(|p| p.language_editions.clone())
			.unwrap_or_default(),
		own_lang: product
			.and_then(|p| p.translation_info)
			.and_then(|t| t.lang),
	})
}
