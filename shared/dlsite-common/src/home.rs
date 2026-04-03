use aidoku::{
	alloc::{format, String, Vec},
	imports::{html::Html, net::Request, std::print},
	Result,
};

use crate::explore::{self, ExploreResult, ExploreSort, ExploreWork};
use crate::settings::{self, DlsiteLang};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const USER_AGENT: &str =
	"Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15";

/// Work type codes for each ranking/recommendation category.
const COMIC_WORK_TYPES: &[&str] = &["MNG", "SCM", "WBT", "ICG", "NRE", "VCM", "ET3"];
const GAME_WORK_TYPES: &[&str] = &[
	"ACN", "QIZ", "ADV", "RPG", "TBL", "SLN", "TYP", "STG", "PZL", "ETC", "MOV", "DNV",
];
const VOICE_WORK_TYPES: &[&str] = &["SOU", "MUS"];

// ---------------------------------------------------------------------------
// Site-aware helpers
// ---------------------------------------------------------------------------

pub fn sex_category_for_site(site_slug: &str) -> &'static str {
	match site_slug {
		"girls" | "bl" => "female",
		_ => "male",
	}
}

pub fn work_categories_for_site(site_slug: &str) -> &'static [&'static str] {
	match site_slug {
		"home" | "maniax" => &["doujin", "pc", "app"],
		"soft" | "pro" => &["pc"],
		"books" => &["books"],
		"girls" | "bl" => &["doujin"],
		_ => &["doujin"],
	}
}

pub fn default_age_category(is_r18: bool) -> &'static str {
	if is_r18 { "adult" } else { "general" }
}

/// Check if any of the user's enabled work types overlap with a category.
fn category_enabled(user_work_types: &[String], category_types: &[&str]) -> bool {
	if user_work_types.is_empty() {
		return true; // no filter = show all
	}
	user_work_types
		.iter()
		.any(|wt| category_types.contains(&wt.as_str()))
}

// ---------------------------------------------------------------------------
// FSR home URL builder
// ---------------------------------------------------------------------------

fn build_home_fsr_url(
	site_slug: &str,
	page: i32,
	sort: ExploreSort,
	sex_category: Option<&str>,
	work_categories: &[&str],
	age_category: &str,
	languages: &[DlsiteLang],
) -> String {
	let base = format!("https://www.dlsite.com/{}", site_slug);
	let mut path = String::from("/fsr/ajax/=/language/jp/ana_flg/all");

	// Age category
	path.push_str(&format!("/age_category%5B0%5D/{}", age_category));

	// Sex category
	if let Some(sex) = sex_category {
		path.push_str(&format!("/sex_category/{}", sex));
	}

	// Work categories (indexed array)
	for (i, cat) in work_categories.iter().enumerate() {
		path.push_str(&format!("/work_category%5B{}%5D/{}", i, cat));
	}

	// Language options
	if !languages.is_empty() {
		path.push_str("/options_and_or/and");
		for (i, lang) in languages.iter().enumerate() {
			path.push_str(&format!("/options%5B{}%5D/{}", i, lang.api_code()));
		}
	}

	// Sort order
	path.push_str(&format!("/order%5B0%5D/{}", sort.order_param()));

	// Page
	path.push_str(&format!("/page/{}", page));

	format!("{}{}", base, path)
}

// ---------------------------------------------------------------------------
// Helper: make a GET request with standard headers
// ---------------------------------------------------------------------------

fn get_request(url: &str) -> Result<Vec<u8>> {
	let cookie_header = settings::get_locale_cookie_header();
	let resp = Request::get(url)?
		.header("Accept", "application/json")
		.header("User-Agent", USER_AGENT)
		.header("Cookie", &cookie_header)
		.send()?;

	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		print(format!("[dlsite-home] HTTP {} for {}", status, url));
		return Ok(Vec::new());
	}
	Ok(data)
}

fn get_html_request(url: &str) -> Result<Vec<u8>> {
	let cookie_header = settings::get_locale_cookie_header();
	let mut req = Request::get(url)?;
	req = req
		.header("Accept", "text/html")
		.header("User-Agent", USER_AGENT)
		.header("Cookie", &cookie_header);

	let resp = req.send()?;
	let status = resp.status_code();
	let data = resp.get_data()?;
	if !(200..300).contains(&status) {
		print(format!("[dlsite-home] HTTP {} for {}", status, url));
		return Ok(Vec::new());
	}
	Ok(data)
}

// ---------------------------------------------------------------------------
// Section 1: Top English Picks
// ---------------------------------------------------------------------------

pub fn fetch_english_picks(site_slug: &str, is_r18: bool, page: i32) -> Result<ExploreResult> {
	let age = default_age_category(is_r18);
	let sex = sex_category_for_site(site_slug);
	let url = build_home_fsr_url(
		site_slug,
		page,
		ExploreSort::Trending,
		Some(sex),
		&["doujin"],
		age,
		&[DlsiteLang::ENG],
	);
	print(format!("[dlsite-home] english_picks → GET {}", url));

	let data = get_request(&url)?;
	if data.is_empty() {
		return Ok(ExploreResult {
			works: Vec::new(),
			has_next_page: false,
		});
	}

	let result = explore::parse_fsr_ajax_response(&data, page)?;
	print(format!(
		"[dlsite-home] english_picks: {} works, has_next={}",
		result.works.len(),
		result.has_next_page
	));
	Ok(result)
}

// ---------------------------------------------------------------------------
// Section 2: Translators Unite Translations
// ---------------------------------------------------------------------------

pub fn fetch_translations(site_slug: &str, page: i32) -> Result<ExploreResult> {
	let url = format!(
		"https://www.dlsite.com/{}/works/translation/ajax?page={}",
		site_slug, page
	);
	print(format!("[dlsite-home] translations → GET {}", url));

	let data = get_request(&url)?;
	if data.is_empty() {
		return Ok(ExploreResult {
			works: Vec::new(),
			has_next_page: false,
		});
	}

	let result = explore::parse_fsr_ajax_response(&data, page)?;
	print(format!(
		"[dlsite-home] translations: {} works, has_next={}",
		result.works.len(),
		result.has_next_page
	));
	Ok(result)
}

// ---------------------------------------------------------------------------
// Section 3: Doujin Ranking (7 Days)
// ---------------------------------------------------------------------------

fn parse_ranking_page(html_bytes: &[u8], _site_slug: &str) -> Vec<ExploreWork> {
	let html_str = match core::str::from_utf8(html_bytes) {
		Ok(s) => s,
		Err(_) => return Vec::new(),
	};
	let doc = match Html::parse(html_str) {
		Ok(d) => d,
		Err(_) => return Vec::new(),
	};

	let rows = match doc.select("table#ranking_table tr") {
		Some(r) => r,
		None => return Vec::new(),
	};

	let mut works = Vec::new();
	for row in rows {
		// Extract product ID and URL from link href
		let product_link = row.select_first("dt.work_name a");
		let (workno, url) = match &product_link {
			Some(a) => match a.attr("href") {
				Some(href) => {
					// Extract RJ/VJ/BJ ID from URL
					if let Some(start) = href.find("product_id/") {
						let after = &href[start + 11..];
						if let Some(end) = after.find('.') {
							(String::from(&after[..end]), Some(href))
						} else {
							continue;
						}
					} else {
						continue;
					}
				}
				None => continue,
			},
			None => continue,
		};

		let title = product_link
			.as_ref()
			.and_then(|a| a.text())
			.unwrap_or_else(|| workno.clone());

		let cover_url =
			explore::extract_thumb_url(&row).or_else(|| explore::cover_url_from_id(&workno));

		let maker_name = row
			.select_first("dd.maker_name a")
			.and_then(|a| a.text());

		// Work type from work_category class (e.g., "work_category type_MNG")
		let work_type = row
			.select_first("div[class*='work_category']")
			.and_then(|div| div.attr("class"))
			.and_then(|cls| {
				cls.split_whitespace()
					.find(|c| c.starts_with("type_"))
					.map(|c| String::from(&c[5..]))
			});

		// Age category: on all-ages sites it's always general, on adult sites always adult.
		// Parse from __product_attributes if present, but it may not have age in position 1
		// on ranking pages. Since site determines the age category, we skip parsing it here.
		let age_category = None;

		works.push(ExploreWork {
			workno,
			title,
			cover_url,
			url,
			maker_name,
			work_type,
			age_category,
		});
	}
	works
}

pub fn fetch_ranking(
	site_slug: &str,
	work_types: &[String],
) -> Result<ExploreResult> {
	let categories: &[&str] = &["comic", "game", "voice"];
	let category_work_types: &[&[&str]] =
		&[COMIC_WORK_TYPES, GAME_WORK_TYPES, VOICE_WORK_TYPES];

	let mut all_works: Vec<ExploreWork> = Vec::new();

	for (cat, cat_types) in categories.iter().zip(category_work_types.iter()) {
		if !category_enabled(work_types, cat_types) {
			continue;
		}

		let url = format!(
			"https://www.dlsite.com/{}/ranking/week?category={}",
			site_slug, cat
		);
		print(format!("[dlsite-home] ranking({}) → GET {}", cat, url));

		let data = get_html_request(&url)?;
		if data.is_empty() {
			continue;
		}

		let works = parse_ranking_page(&data, site_slug);
		print(format!(
			"[dlsite-home] ranking({}): {} works",
			cat,
			works.len()
		));
		all_works.extend(works);
	}

	Ok(ExploreResult {
		works: all_works,
		has_next_page: false,
	})
}

// ---------------------------------------------------------------------------
// Section 4: Recommended (from /load/recommend/parts API)
// ---------------------------------------------------------------------------

#[derive(aidoku::serde::Deserialize)]
struct RecommendPart {
	#[serde(default)]
	html: String,
}

fn parse_recommend_html(html: &str) -> Vec<ExploreWork> {
	let doc = match Html::parse_fragment(html) {
		Ok(d) => d,
		Err(_) => return Vec::new(),
	};

	let items = match doc.select("div.recommend_work_item") {
		Some(i) => i,
		None => return Vec::new(),
	};

	let mut works = Vec::new();

	for item in items {
		let data_div = item.select_first("div[data-product_id]");
		let workno = match &data_div {
			Some(div) => match div.attr("data-product_id") {
				Some(id) if !id.is_empty() => id,
				_ => continue,
			},
			None => continue,
		};

		let title = data_div
			.as_ref()
			.and_then(|div| div.attr("data-work_name"))
			.unwrap_or_else(|| workno.clone());

		let work_type = data_div
			.as_ref()
			.and_then(|div| div.attr("data-work_type"));

		let cover_url =
			explore::extract_thumb_url(&item).or_else(|| explore::cover_url_from_id(&workno));

		let url = item
			.select_first("a.work_thumb")
			.and_then(|a| a.attr("href"));

		let maker_name = item
			.select_first("div.maker_name a")
			.and_then(|a| a.text());

		let age_category = item
			.select_first("input.__product_attributes")
			.and_then(|input| input.attr("value"))
			.and_then(|v| explore::parse_age_from_attributes(&v));

		works.push(ExploreWork {
			workno,
			title,
			cover_url,
			url,
			maker_name,
			work_type,
			age_category,
		});
	}

	works
}

pub fn fetch_recommended(site_slug: &str, recommend_type: &str) -> Result<ExploreResult> {
	let url = format!(
		"https://www.dlsite.com/{}/load/recommend/parts/=/type/{}/id/1",
		site_slug, recommend_type
	);
	print(format!("[dlsite-home] recommended({}) → GET {}", recommend_type, url));

	let data = get_request(&url)?;
	if data.is_empty() {
		return Ok(ExploreResult {
			works: Vec::new(),
			has_next_page: false,
		});
	}

	let parts: Vec<RecommendPart> = serde_json::from_slice(&data).unwrap_or_default();
	let mut works = Vec::new();
	for part in &parts {
		works.extend(parse_recommend_html(&part.html));
	}

	print(format!(
		"[dlsite-home] recommended({}): {} works",
		recommend_type,
		works.len()
	));

	Ok(ExploreResult {
		works,
		has_next_page: false,
	})
}

// ---------------------------------------------------------------------------
// Section 5: New doujin works
// ---------------------------------------------------------------------------

pub fn fetch_new_works(
	site_slug: &str,
	is_r18: bool,
	languages: &[DlsiteLang],
	page: i32,
) -> Result<ExploreResult> {
	let age = default_age_category(is_r18);
	let sex = sex_category_for_site(site_slug);
	let work_cats = work_categories_for_site(site_slug);
	let url = build_home_fsr_url(
		site_slug,
		page,
		ExploreSort::Newest,
		Some(sex),
		work_cats,
		age,
		languages,
	);
	print(format!("[dlsite-home] new_works → GET {}", url));

	let data = get_request(&url)?;
	if data.is_empty() {
		return Ok(ExploreResult {
			works: Vec::new(),
			has_next_page: false,
		});
	}

	let result = explore::parse_fsr_ajax_response(&data, page)?;
	print(format!(
		"[dlsite-home] new_works: {} works, has_next={}",
		result.works.len(),
		result.has_next_page
	));
	Ok(result)
}

// ---------------------------------------------------------------------------
// Section 6: Popular doujin works
// ---------------------------------------------------------------------------

pub fn fetch_popular_works(
	site_slug: &str,
	is_r18: bool,
	languages: &[DlsiteLang],
	page: i32,
) -> Result<ExploreResult> {
	let age = default_age_category(is_r18);
	let sex = sex_category_for_site(site_slug);
	let work_cats = work_categories_for_site(site_slug);
	let url = build_home_fsr_url(
		site_slug,
		page,
		ExploreSort::Trending,
		Some(sex),
		work_cats,
		age,
		languages,
	);
	print(format!("[dlsite-home] popular_works → GET {}", url));

	let data = get_request(&url)?;
	if data.is_empty() {
		return Ok(ExploreResult {
			works: Vec::new(),
			has_next_page: false,
		});
	}

	let result = explore::parse_fsr_ajax_response(&data, page)?;
	print(format!(
		"[dlsite-home] popular_works: {} works, has_next={}",
		result.works.len(),
		result.has_next_page
	));
	Ok(result)
}
