use crate::models::PublicWork;
use aidoku::{
	alloc::{format, String, Vec},
	imports::{net::Request, std::print},
	Result,
};

const DLSITE_BASE: &str = "https://www.dlsite.com/maniax";

/// Fetch work details from the public DLsite product JSON API.
/// Used as a fallback when viewing a work that isn't purchased.
pub fn get_public_work_details(workno: &str, locale: Option<&str>) -> Result<Option<PublicWork>> {
	let url = match locale {
		Some(loc) => format!(
			"{}/api/=/product.json?workno={}&locale={}",
			DLSITE_BASE, workno, loc
		),
		None => format!(
			"{}/api/=/product.json?workno={}",
			DLSITE_BASE, workno
		),
	};
	print(format!("[dlsite] public detail → GET {}", url));

	let resp = Request::get(&url)?
		.header("Accept", "application/json")
		.send()?;

	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		print(format!(
			"[dlsite] public detail HTTP {} for {}",
			status, workno
		));
		return Ok(None);
	}

	let products: Vec<PublicWork> = serde_json::from_slice(&data).unwrap_or_default();
	Ok(products.into_iter().next())
}
