use aidoku::{
	alloc::{format, String, Vec},
	imports::{html::Html, net::Request, std::print},
	ContentRating, Manga, Result,
};

use crate::settings::DlsiteLang;

// ---------------------------------------------------------------------------
// Explore sort options (server-side via /fsr/ajax/=/)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExploreSort {
	Newest = 0,
	Trending = 1,
	Downloads = 2,
	Rating = 3,
}

impl ExploreSort {
	pub fn from_index(index: i32) -> Self {
		match index {
			1 => Self::Trending,
			2 => Self::Downloads,
			3 => Self::Rating,
			_ => Self::Newest,
		}
	}

	/// DLsite `order` path segment value.
	pub fn order_param(self) -> &'static str {
		match self {
			Self::Newest => "release_d",
			Self::Trending => "trend",
			Self::Downloads => "dl_d",
			Self::Rating => "rate_d",
		}
	}
}

// ---------------------------------------------------------------------------
// Search result models (parsed from /fsr/ajax/=/ HTML)
// ---------------------------------------------------------------------------

pub struct ExploreWork {
	pub workno: String,
	pub title: String,
	pub cover_url: Option<String>,
	/// Full URL to the work page, extracted from HTML. When present, preferred
	/// over the default `/work/=/product_id/` URL (e.g. for pre-release works
	/// whose pages live under `/announce/=`).
	pub url: Option<String>,
	pub maker_name: Option<String>,
	pub work_type: Option<String>,
	/// Raw age string from `__product_attributes`: `"adl"`, `"r15"`, or absent.
	pub age_category: Option<String>,
}

pub struct ExploreResult {
	pub works: Vec<ExploreWork>,
	pub has_next_page: bool,
}

impl ExploreWork {
	fn work_type_label(&self) -> Option<&'static str> {
		match self.work_type.as_deref()? {
			"MNG" => Some("Manga"),
			"SCM" => Some("Gekiga"),
			"WBT" => Some("Webtoon"),
			"ICG" => Some("CG / Illustration"),
			"NRE" => Some("Novel"),
			"DNV" => Some("Digital Novel"),
			"MOV" => Some("Video"),
			"SOU" => Some("Sound / Voice"),
			"MUS" => Some("Music"),
			"ACN" | "QIZ" | "ADV" | "RPG" | "TBL" | "SLN" | "TYP" | "STG" | "PZL" => {
				Some("Game")
			}
			"ETC" | "ET3" => Some("Other"),
			"TOL" => Some("Tools / Accessories"),
			"IMT" => Some("Illustration Materials"),
			"AMT" => Some("Music Materials"),
			"VCM" => Some("Voiced Comic"),
			"PBC" => Some("Publication"),
			_ => None,
		}
	}

	pub fn into_manga(self, site_slug: &str) -> Manga {
		let content_rating = match self.age_category.as_deref() {
			Some("adl") => ContentRating::NSFW,
			Some("r15") => ContentRating::Suggestive,
			_ => ContentRating::Safe,
		};

		let mut tags: Vec<String> = Vec::new();
		if let Some(label) = self.work_type_label() {
			tags.push(label.into());
		}

		let description = self.maker_name.as_ref().map(|m| format!("Circle: {}", m));

		let url = self.url.or_else(|| {
			Some(format!(
				"https://www.dlsite.com/{}/work/=/product_id/{}.html",
				site_slug, self.workno
			))
		});

		Manga {
			key: format!("{}/{}", site_slug, self.workno),
			title: self.title,
			cover: self.cover_url,
			description,
			tags: if tags.is_empty() { None } else { Some(tags) },
			content_rating,
			url,
			..Default::default()
		}
	}
}

// ---------------------------------------------------------------------------
// Search URL builder
// ---------------------------------------------------------------------------

/// DLsite ignores per_page and always returns 30 items.
const EXPLORE_PAGE_SIZE: i32 = 30;

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

/// Build a `/fsr/ajax/=/` search URL from the given parameters.
pub fn build_search_url(
	site_slug: &str,
	keyword: Option<&str>,
	page: i32,
	sort: ExploreSort,
	languages: &[DlsiteLang],
	work_types: &[String],
	content_ratings: &[String],
	genres: &[u32],
) -> String {
	let base = format!("https://www.dlsite.com/{}", site_slug);
	let mut path = String::from("/fsr/ajax/=/language/jp");

	// Language options — uses options[N] indexed array params
	if !languages.is_empty() {
		path.push_str("/options_and_or/and");
		for (i, lang) in languages.iter().enumerate() {
			path.push_str(&format!("/options%5B{}%5D/{}", i, lang.api_code()));
		}
	}

	// Content ratings — use indexed array params
	let mut cr_idx = 0usize;
	for cr in content_ratings {
		let api_val = match cr.as_str() {
			"safe" => "general",
			"r15" => "r15",
			"r18" => "adult",
			_ => continue,
		};
		path.push_str(&format!("/age_category%5B{}%5D/{}", cr_idx, api_val));
		cr_idx += 1;
	}

	// Work types — use indexed array params
	for (i, wt) in work_types.iter().enumerate() {
		path.push_str(&format!("/work_type%5B{}%5D/{}", i, wt));
	}

	// Genres — use indexed array params
	for (i, gid) in genres.iter().enumerate() {
		path.push_str(&format!("/genre%5B{}%5D/{}", i, gid));
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

	format!("{}{}", base, path)
}

// ---------------------------------------------------------------------------
// Key helpers
// ---------------------------------------------------------------------------

/// Split an explore manga key (`"site_slug/product_id"`) into its components.
/// Falls back to `default_slug` if the key has no slash.
pub fn split_key<'a>(key: &'a str, default_slug: &'a str) -> (&'a str, &'a str) {
	key.split_once('/').unwrap_or((default_slug, key))
}

// ---------------------------------------------------------------------------
// HTML parsing
// ---------------------------------------------------------------------------

/// Construct the cover thumbnail URL from a product ID.
///
/// Pattern: `//img.dlsite.jp/resize/images2/work/{category}/{bucket}/{id}_img_main_240x240.jpg`
/// - category: doujin (RJ), professional (VJ), books (BJ)
/// - bucket: product ID rounded up to nearest 1000
pub fn cover_url_from_id(workno: &str) -> Option<String> {
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
pub fn parse_age_from_attributes(attrs: &str) -> Option<String> {
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

/// Extract cover image URL from Vue image component attributes.
///
/// Checks two component types used across DLsite pages:
/// - `thumb-with-ng-filter-block` with `:thumb-candidates` (FSR search results)
/// - `img-with-fallback` with `:candidates` (recommended section HTML)
///
/// Both use the same array format:
/// `['//img.dlsite.jp/...240x240.webp','//img.dlsite.jp/...240x240.jpg']`
///
/// Prefers the `.jpg` entry; normalizes protocol-relative URLs to `https:`.
pub fn extract_thumb_url(element: &aidoku::imports::html::Element) -> Option<String> {
	let candidates = element
		.select_first("thumb-with-ng-filter-block")
		.and_then(|el| el.attr(":thumb-candidates"))
		.or_else(|| {
			element
				.select_first("img-with-fallback")
				.and_then(|el| el.attr(":candidates"))
		})?;
	let url = candidates.split('\'').find(|s| s.ends_with(".jpg"))?;
	Some(if url.starts_with("//") {
		format!("https:{}", url)
	} else {
		String::from(url)
	})
}

/// Parse a single `<li>` product element into an `ExploreWork`.
pub fn parse_product_element(li: &aidoku::imports::html::Element) -> Option<ExploreWork> {
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

	// Cover URL: prefer HTML-embedded URL (handles translations + pre-release),
	// fall back to constructing from product ID
	let cover_url = extract_thumb_url(li).or_else(|| cover_url_from_id(&workno));

	// Circle/maker name
	let maker_name = li
		.select_first("dd.maker_name a")
		.and_then(|a| a.text());

	// Work type from data-worktype attribute on SampleViewMiniButton
	let work_type = li
		.select_first("span[data-worktype]")
		.and_then(|span| span.attr("data-worktype"));

	// URL from the thumb link (handles announce/pre-release pages correctly)
	let url = li
		.select_first("thumb-with-ng-filter-block")
		.and_then(|el| el.attr("link"));

	// Age category from hidden __product_attributes input
	let age_category = li
		.select_first("input.__product_attributes")
		.and_then(|input| input.attr("value"))
		.and_then(|v| parse_age_from_attributes(&v));

	Some(ExploreWork {
		workno,
		title,
		cover_url,
		url,
		maker_name,
		work_type,
		age_category,
	})
}

// ---------------------------------------------------------------------------
// Public search function
// ---------------------------------------------------------------------------

/// Parse a `/fsr/ajax/=/` JSON response (containing an HTML fragment) into
/// an `ExploreResult`. Reusable by both search and home sections.
pub fn parse_fsr_ajax_response(data: &[u8], page: i32) -> Result<ExploreResult> {
	let ajax: SearchAjaxResponse = serde_json::from_slice(data)?;

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

	Ok(ExploreResult {
		works,
		has_next_page,
	})
}

/// Search DLsite's public catalog via the `/fsr/ajax/=/` endpoint.
pub fn search_explore(
	site_slug: &str,
	keyword: Option<&str>,
	page: i32,
	sort: ExploreSort,
	languages: &[DlsiteLang],
	work_types: &[String],
	content_ratings: &[String],
	genres: &[u32],
) -> Result<ExploreResult> {
	let url = build_search_url(
		site_slug,
		keyword,
		page,
		sort,
		languages,
		work_types,
		content_ratings,
		genres,
	);
	print(format!("[dlsite-explore] → GET {}", url));

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
			"[dlsite-explore] HTTP {} ({} bytes)",
			status,
			data.len()
		));
		return Ok(ExploreResult {
			works: Vec::new(),
			has_next_page: false,
		});
	}

	let result = parse_fsr_ajax_response(&data, page)?;

	print(format!(
		"[dlsite-explore] {} works, has_next={}",
		result.works.len(),
		result.has_next_page
	));

	Ok(result)
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku::alloc::vec;

	#[test]
	fn cover_url_rj_product() {
		let url = cover_url_from_id("RJ01599911").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/doujin/RJ01600000/RJ01599911_img_main_240x240.jpg"
		);
	}

	#[test]
	fn cover_url_vj_product() {
		let url = cover_url_from_id("VJ01006082").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/professional/VJ01007000/VJ01006082_img_main_240x240.jpg"
		);
	}

	#[test]
	fn cover_url_bj_product() {
		let url = cover_url_from_id("BJ02452708").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/books/BJ02453000/BJ02452708_img_main_240x240.jpg"
		);
	}

	#[test]
	fn cover_url_exact_thousand() {
		let url = cover_url_from_id("RJ01000000").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/doujin/RJ01000000/RJ01000000_img_main_240x240.jpg"
		);
	}

	#[test]
	fn parse_age_adl() {
		assert_eq!(
			parse_age_from_attributes("RG45215,adl,male,ICG,JPN"),
			Some("adl".into())
		);
	}

	#[test]
	fn parse_age_r15() {
		assert_eq!(
			parse_age_from_attributes("RG12345,r15,male,MNG,JPN"),
			Some("r15".into())
		);
	}

	#[test]
	fn parse_age_general() {
		assert_eq!(
			parse_age_from_attributes("RG12345,general,male,SOU,JPN"),
			Some("general".into())
		);
	}

	#[test]
	fn parse_age_empty() {
		assert_eq!(parse_age_from_attributes(""), None);
	}

	#[test]
	fn build_search_url_basic() {
		let url = build_search_url("maniax", None, 1, ExploreSort::Newest, &[], &[], &[], &[]);
		assert_eq!(
			url,
			"https://www.dlsite.com/maniax/fsr/ajax/=/language/jp/order%5B0%5D/release_d/page/1"
		);
	}

	#[test]
	fn build_search_url_different_site() {
		let url = build_search_url("home", None, 1, ExploreSort::Trending, &[], &[], &[], &[]);
		assert_eq!(
			url,
			"https://www.dlsite.com/home/fsr/ajax/=/language/jp/order%5B0%5D/trend/page/1"
		);
	}

	#[test]
	fn build_search_url_with_single_language() {
		let langs = vec![DlsiteLang::ENG];
		let url = build_search_url("maniax", None, 1, ExploreSort::Newest, &langs, &[], &[], &[]);
		assert!(url.contains("/language/jp"));
		assert!(url.contains("/options_and_or/and"));
		assert!(url.contains("/options%5B0%5D/ENG"));
	}

	#[test]
	fn build_search_url_with_multiple_languages() {
		let langs = vec![DlsiteLang::JPN, DlsiteLang::ENG];
		let url = build_search_url("maniax", None, 1, ExploreSort::Newest, &langs, &[], &[], &[]);
		assert!(url.contains("/language/jp"));
		assert!(url.contains("/options_and_or/and"));
		assert!(url.contains("/options%5B0%5D/JPN"));
		assert!(url.contains("/options%5B1%5D/ENG"));
	}

	#[test]
	fn build_search_url_with_filters() {
		let types = vec!["MNG".into(), "WBT".into()];
		let ratings = vec!["r18".into()];
		let url = build_search_url(
			"maniax",
			Some("test"),
			2,
			ExploreSort::Trending,
			&[],
			&types,
			&ratings,
			&[],
		);
		assert!(url.contains("/age_category%5B0%5D/adult"));
		assert!(url.contains("/work_type%5B0%5D/MNG"));
		assert!(url.contains("/work_type%5B1%5D/WBT"));
		assert!(url.contains("/keyword/test"));
		assert!(url.contains("/order%5B0%5D/trend"));
		assert!(url.contains("/page/2"));
	}

	#[test]
	fn build_search_url_with_multiple_ratings() {
		let ratings = vec!["safe".into(), "r15".into()];
		let url =
			build_search_url("maniax", None, 1, ExploreSort::Newest, &[], &[], &ratings, &[]);
		assert!(url.contains("/age_category%5B0%5D/general"));
		assert!(url.contains("/age_category%5B1%5D/r15"));
	}

	#[test]
	fn build_search_url_with_genres() {
		let url =
			build_search_url("maniax", None, 1, ExploreSort::Newest, &[], &[], &[], &[509, 66]);
		assert!(url.contains("/genre%5B0%5D/509"));
		assert!(url.contains("/genre%5B1%5D/66"));
	}
}
