use super::models::{ExploreResult, ExploreSort, ExploreWork, PublicWork};
use aidoku::{
	alloc::{format, String, Vec},
	imports::{html::Html, net::Request, std::print},
	Result,
};

const DLSITE_BASE: &str = "https://www.dlsite.com/maniax";
/// DLsite ignores per_page and always returns 30 items.
const EXPLORE_PAGE_SIZE: i32 = 30;

// ---------------------------------------------------------------------------
// Search URL builder
// ---------------------------------------------------------------------------

/// Build a `/fsr/ajax/=/` search URL from the given parameters.
fn build_search_url(
	keyword: Option<&str>,
	page: i32,
	sort: ExploreSort,
	work_types: &[String],
	content_rating: Option<&str>,
) -> String {
	let mut path = String::from("/fsr/ajax/=/language/jp");

	// Content rating
	match content_rating {
		Some("safe") => path.push_str("/age_category%5B0%5D/general"),
		Some("r15") => path.push_str("/age_category%5B0%5D/r15"),
		Some("r18") => path.push_str("/age_category%5B0%5D/adult"),
		_ => {} // "all" or None — omit to get all ratings
	}

	// Work types — use indexed array params
	for (i, wt) in work_types.iter().enumerate() {
		path.push_str(&format!("/work_type%5B{}%5D/{}", i, wt));
	}

	// Keyword
	if let Some(kw) = keyword {
		if !kw.is_empty() {
			path.push_str(&format!("/keyword/{}", kw));
		}
	}

	// Sort order
	path.push_str(&format!("/order%5B0%5D/{}", sort.order_param()));

	// Page
	path.push_str(&format!("/page/{}", page));

	format!("{}{}", DLSITE_BASE, path)
}

// ---------------------------------------------------------------------------
// HTML parsing
// ---------------------------------------------------------------------------

/// JSON wrapper returned by `/fsr/ajax/=/`.
#[derive(aidoku::serde::Deserialize)]
struct SearchAjaxResponse {
	#[serde(default)]
	search_result: String,
	#[serde(default)]
	page_info: SearchPageInfo,
}

#[derive(aidoku::serde::Deserialize, Default)]
struct SearchPageInfo {
	#[serde(default)]
	count: i64,
}

/// Construct the cover thumbnail URL from a product ID.
///
/// Pattern: `//img.dlsite.jp/resize/images2/work/{category}/{bucket}/{id}_img_main_240x240.jpg`
/// - category: doujin (RJ), professional (VJ), books (BJ)
/// - bucket: product ID rounded up to nearest 1000
fn cover_url_from_id(workno: &str) -> Option<String> {
	let category = if workno.starts_with("RJ") {
		"doujin"
	} else if workno.starts_with("VJ") {
		"professional"
	} else if workno.starts_with("BJ") {
		"books"
	} else {
		"doujin"
	};

	// Extract numeric part and compute bucket (round up to nearest 1000)
	let prefix = &workno[..2];
	let digits = &workno[2..];
	let width = digits.len();
	let num: i64 = digits.parse().ok()?;
	let bucket_num = ((num + 999) / 1000) * 1000;
	let bucket = format!("{}{:0>width$}", prefix, bucket_num, width = width);

	Some(format!(
		"https://img.dlsite.jp/resize/images2/work/{}/{}/{}_img_main_240x240.jpg",
		category, bucket, workno
	))
}

/// Parse age category from the hidden `__product_attributes` CSV value.
///
/// Example value: `RG45215,adl,male,ICG,JPN,DLP,504,536,...`
/// The age field is typically the 2nd element: `adl`, `r15`, or `general`.
fn parse_age_from_attributes(attrs: &str) -> Option<String> {
	let parts: Vec<&str> = attrs.split(',').collect();
	if parts.len() >= 2 {
		let age = parts[1];
		match age {
			"adl" | "r15" | "general" => Some(age.into()),
			_ => None,
		}
	} else {
		None
	}
}

/// Parse a single `<li>` product element into an `ExploreWork`.
fn parse_product_element(li: &aidoku::imports::html::Element) -> Option<ExploreWork> {
	let workno = li.attr("data-list_item_product_id")?;
	if workno.is_empty() {
		return None;
	}

	// Title: from <dd class="work_name"> > a
	let title = li
		.select_first("dd.work_name a")
		.and_then(|a| {
			let t = a.attr("title");
			if t.is_some() && !t.as_ref().unwrap().is_empty() {
				t
			} else {
				a.text()
			}
		})
		.unwrap_or_else(|| workno.clone());

	// Cover URL: constructed from product ID
	let cover_url = cover_url_from_id(&workno);

	// Circle/maker name
	let maker_name = li
		.select_first("dd.maker_name a")
		.and_then(|a| a.text());

	// Work type from data-worktype attribute on SampleViewMiniButton
	let work_type = li
		.select_first("span[data-worktype]")
		.and_then(|span| span.attr("data-worktype"));

	// Age category from hidden __product_attributes input
	let age_category = li
		.select_first("input.__product_attributes")
		.and_then(|input| input.attr("value"))
		.and_then(|v| parse_age_from_attributes(&v));

	Some(ExploreWork {
		workno,
		title,
		cover_url,
		maker_name,
		work_type,
		age_category,
	})
}

// ---------------------------------------------------------------------------
// Public search function
// ---------------------------------------------------------------------------

/// Search DLsite's public catalog via the `/fsr/ajax/=/` endpoint.
pub fn search_explore(
	keyword: Option<&str>,
	page: i32,
	sort: ExploreSort,
	work_types: &[String],
	content_rating: Option<&str>,
) -> Result<ExploreResult> {
	let url = build_search_url(keyword, page, sort, work_types, content_rating);
	print(format!("[dlsite-play] explore → GET {}", url));

	let resp = Request::get(&url)?
		.header("Accept", "application/json")
		.header(
			"User-Agent",
			"Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15",
		)
		.send()?;

	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		print(format!(
			"[dlsite-play] explore HTTP {} ({} bytes)",
			status,
			data.len()
		));
		return Ok(ExploreResult {
			works: Vec::new(),
			has_next_page: false,
		});
	}

	let ajax: SearchAjaxResponse = serde_json::from_slice(&data)?;

	// Parse the HTML fragment
	let doc = Html::parse_fragment(&ajax.search_result)?;
	let items = doc.select("li[data-list_item_product_id]");

	let mut works: Vec<ExploreWork> = Vec::new();
	if let Some(items) = items {
		for item in items {
			if let Some(work) = parse_product_element(&item) {
				works.push(work);
			}
		}
	}

	let has_next_page = (page as i64) * (EXPLORE_PAGE_SIZE as i64) < ajax.page_info.count;

	print(format!(
		"[dlsite-play] explore: {} works, total={}, has_next={}",
		works.len(),
		ajax.page_info.count,
		has_next_page
	));

	Ok(ExploreResult {
		works,
		has_next_page,
	})
}

// ---------------------------------------------------------------------------
// Public product detail fallback
// ---------------------------------------------------------------------------

/// Fetch work details from the public DLsite product JSON API.
/// Used as a fallback when a user taps an explore result that isn't purchased.
pub fn get_public_work_details(workno: &str) -> Result<Option<PublicWork>> {
	let url = format!(
		"{}/api/=/product.json?workno={}",
		DLSITE_BASE, workno
	);
	print(format!("[dlsite-play] explore detail → GET {}", url));

	let resp = Request::get(&url)?
		.header("Accept", "application/json")
		.send()?;

	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		print(format!(
			"[dlsite-play] explore detail HTTP {} for {}",
			status, workno
		));
		return Ok(None);
	}

	let products: Vec<PublicWork> = serde_json::from_slice(&data).unwrap_or_default();
	Ok(products.into_iter().next())
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku::alloc::vec;
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn cover_url_rj_product() {
		let url = cover_url_from_id("RJ01599911").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/doujin/RJ01600000/RJ01599911_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn cover_url_vj_product() {
		let url = cover_url_from_id("VJ01006082").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/professional/VJ01007000/VJ01006082_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn cover_url_bj_product() {
		let url = cover_url_from_id("BJ02452708").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/books/BJ02453000/BJ02452708_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn cover_url_exact_thousand() {
		let url = cover_url_from_id("RJ01000000").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/doujin/RJ01000000/RJ01000000_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn parse_age_adl() {
		assert_eq!(
			parse_age_from_attributes("RG45215,adl,male,ICG,JPN"),
			Some("adl".into())
		);
	}

	#[aidoku_test]
	fn parse_age_r15() {
		assert_eq!(
			parse_age_from_attributes("RG12345,r15,male,MNG,JPN"),
			Some("r15".into())
		);
	}

	#[aidoku_test]
	fn parse_age_general() {
		assert_eq!(
			parse_age_from_attributes("RG12345,general,male,SOU,JPN"),
			Some("general".into())
		);
	}

	#[aidoku_test]
	fn parse_age_empty() {
		assert_eq!(parse_age_from_attributes(""), None);
	}

	#[aidoku_test]
	fn build_search_url_basic() {
		let url = build_search_url(None, 1, ExploreSort::Newest, &[], None);
		assert_eq!(
			url,
			"https://www.dlsite.com/maniax/fsr/ajax/=/language/jp/order%5B0%5D/release_d/page/1"
		);
	}

	#[aidoku_test]
	fn build_search_url_with_filters() {
		let types = vec!["MNG".into(), "WBT".into()];
		let url = build_search_url(Some("test"), 2, ExploreSort::Trending, &types, Some("r18"));
		assert!(url.contains("/age_category%5B0%5D/adult"));
		assert!(url.contains("/work_type%5B0%5D/MNG"));
		assert!(url.contains("/work_type%5B1%5D/WBT"));
		assert!(url.contains("/keyword/test"));
		assert!(url.contains("/order%5B0%5D/trend"));
		assert!(url.contains("/page/2"));
	}
}
