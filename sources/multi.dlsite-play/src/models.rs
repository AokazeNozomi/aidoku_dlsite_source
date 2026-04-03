use aidoku::{
	ContentRating, Manga, Viewer,
	alloc::{String, Vec, collections::BTreeMap, format, vec},
	serde::Deserialize,
};

// ---------------------------------------------------------------------------
// Purchase data from POST /api/v3/content/works
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct LocalizedName {
	#[serde(default)]
	pub ja_JP: Option<String>,
	#[serde(default)]
	pub en_US: Option<String>,
	#[serde(default)]
	pub zh_CN: Option<String>,
	#[serde(default)]
	pub zh_TW: Option<String>,
	#[serde(default)]
	pub ko_KR: Option<String>,
	#[serde(default)]
	pub es_ES: Option<String>,
	#[serde(default)]
	pub de_DE: Option<String>,
	#[serde(default)]
	pub fr_FR: Option<String>,
	#[serde(default)]
	pub it_IT: Option<String>,
	#[serde(default)]
	pub pt_BR: Option<String>,
	#[serde(default)]
	pub id_ID: Option<String>,
	#[serde(default)]
	pub vi_VN: Option<String>,
	#[serde(default)]
	pub th_TH: Option<String>,
	#[serde(default)]
	pub sv_SE: Option<String>,
}

impl LocalizedName {
	pub fn best(&self) -> String {
		use crate::settings::Language;
		let primary = match crate::settings::get_preferred_language() {
			Language::English => self.en_US.as_deref(),
			Language::Japanese => self.ja_JP.as_deref(),
			Language::ChineseSimplified => self.zh_CN.as_deref(),
			Language::ChineseTraditional => self.zh_TW.as_deref(),
			Language::Korean => self.ko_KR.as_deref(),
			Language::Spanish => self.es_ES.as_deref(),
			Language::German => self.de_DE.as_deref(),
			Language::French => self.fr_FR.as_deref(),
			Language::Italian => self.it_IT.as_deref(),
			Language::Portuguese => self.pt_BR.as_deref(),
			Language::Indonesian => self.id_ID.as_deref(),
			Language::Vietnamese => self.vi_VN.as_deref(),
			Language::Thai => self.th_TH.as_deref(),
			Language::Swedish => self.sv_SE.as_deref(),
		};
		primary
			.or(self.en_US.as_deref())
			.or(self.ja_JP.as_deref())
			.or(self.zh_CN.as_deref())
			.or(self.zh_TW.as_deref())
			.or(self.ko_KR.as_deref())
			.or(self.es_ES.as_deref())
			.or(self.de_DE.as_deref())
			.or(self.fr_FR.as_deref())
			.or(self.it_IT.as_deref())
			.or(self.pt_BR.as_deref())
			.or(self.id_ID.as_deref())
			.or(self.vi_VN.as_deref())
			.or(self.th_TH.as_deref())
			.or(self.sv_SE.as_deref())
			.unwrap_or("Unknown")
			.into()
	}
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct MakerInfo {
	pub id: String,
	#[serde(default)]
	pub name: Option<LocalizedName>,
	#[serde(default)]
	pub name_phonetic: Option<LocalizedName>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct WorkFilesInfo {
	#[serde(default)]
	pub main: Option<String>,
	#[serde(default)]
	pub sam: Option<String>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct WorkTag {
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default, rename = "class")]
	pub tag_class: Option<String>,
	#[serde(default)]
	pub sub_class: Option<String>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct TranslatorInfo {
	pub id: String,
	#[serde(default)]
	pub name: Option<LocalizedName>,
	#[serde(default)]
	pub name_phonetic: Option<LocalizedName>,
}

#[derive(Deserialize, Clone)]
pub struct WorkSeries {
	pub title_id: String,
	#[serde(default)]
	pub volume_number: Option<u32>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct GenreInfo {
	pub id: u32,
	#[serde(default)]
	pub category_id: Option<u32>,
	#[serde(default)]
	pub sort: Option<u32>,
	#[serde(default)]
	pub name: Option<LocalizedName>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct GenreCategory {
	pub id: u32,
	#[serde(default)]
	pub sort: Option<u32>,
	#[serde(default)]
	pub name: Option<LocalizedName>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct GenresResponse {
	#[serde(default)]
	pub genres: Vec<GenreInfo>,
	#[serde(default)]
	pub categories: Vec<GenreCategory>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct PurchaseWork {
	pub workno: String,
	#[serde(default)]
	pub name: Option<LocalizedName>,
	#[serde(default)]
	pub name_phonetic: Option<LocalizedName>,
	#[serde(default)]
	pub maker: Option<MakerInfo>,
	#[serde(default)]
	pub translator: Option<TranslatorInfo>,
	#[serde(default)]
	pub author_name: Option<String>,
	#[serde(default)]
	pub work_type: Option<String>,
	#[serde(default)]
	pub file_type: Option<String>,
	#[serde(default)]
	pub age_category: Option<String>,
	#[serde(default)]
	pub dl_format: Option<u32>,
	#[serde(default)]
	pub site_id: Option<String>,
	#[serde(default)]
	pub content_length: Option<u64>,
	#[serde(default)]
	pub content_count: Option<u32>,
	#[serde(default)]
	pub content_size: Option<u64>,
	#[serde(default)]
	pub touch_content_count: Option<u32>,
	#[serde(default)]
	pub touch_site_id: Option<String>,
	#[serde(default)]
	pub os: Option<Vec<String>>,
	#[serde(default)]
	pub work_files: Option<WorkFilesInfo>,
	#[serde(default)]
	pub is_playwork: Option<bool>,
	#[serde(default)]
	pub downloadable: Option<bool>,
	#[serde(default)]
	pub encodable: Option<bool>,
	#[serde(default)]
	pub app_type: Option<String>,
	#[serde(default)]
	pub viewer_type: Option<String>,
	#[serde(default)]
	pub tags: Option<Vec<WorkTag>>,
	#[serde(default)]
	pub regist_date: Option<String>,
	#[serde(default)]
	pub upgrade_date: Option<String>,
	#[serde(default)]
	pub sales_date: Option<String>,
	#[serde(default)]
	pub genre_ids: Vec<u32>,
	#[serde(default)]
	pub series: Option<WorkSeries>,
	#[serde(default)]
	pub purchase_type: Option<u32>,
	#[serde(default)]
	pub download_start_date: Option<String>,
}

impl PurchaseWork {
	pub fn has_translator(&self) -> bool {
		self.translator.is_some()
	}

	pub fn cover_url(&self) -> Option<String> {
		self.work_files.as_ref()?.main.as_ref().map(|url| {
			if url.starts_with("//") {
				format!("https:{}", url)
			} else {
				url.clone()
			}
		})
	}

	/// Check if this work's age_category matches any of the given content rating
	/// filter values. Empty slice means "all" (always matches).
	pub fn matches_content_ratings(&self, filters: &[String]) -> bool {
		if filters.is_empty() {
			return true;
		}
		let age = self.age_category.as_deref().unwrap_or("");
		filters.iter().any(|f| match f.as_str() {
			"safe" => !matches!(age, "R18" | "r18" | "R15" | "r15"),
			"r15" => matches!(age, "R15" | "r15"),
			"r18" => matches!(age, "R18" | "r18"),
			_ => false,
		})
	}
}

impl PurchaseWork {
	fn work_type_label(&self) -> Option<&'static str> {
		match self.work_type.as_deref()? {
			"MNG" => Some("Manga"),
			"WBT" => Some("Webtoon"),
			"CG" => Some("CG / Illustration"),
			"SOU" => Some("Sound / Voice"),
			"MOV" => Some("Video"),
			"NOV" => Some("Novel"),
			"GAM" => Some("Game"),
			"ETC" => Some("Other"),
			_ => None,
		}
	}

	/// Extract names from tags matching a given `class` value.
	fn tags_by_class(&self, class: &str) -> Vec<String> {
		self.tags
			.as_ref()
			.map(|tags| {
				tags.iter()
					.filter(|t| t.tag_class.as_deref() == Some(class))
					.filter_map(|t| t.name.clone())
					.collect()
			})
			.unwrap_or_default()
	}

	/// Truncate an ISO-8601 datetime string to just the date portion.
	fn release_date_short(&self) -> Option<&str> {
		let d = self.regist_date.as_deref()?;
		Some(d.get(..10).unwrap_or(d))
	}

	/// Parse release date into unix timestamp (UTC midnight).
	pub fn release_date_timestamp(&self) -> Option<i64> {
		let date = self.release_date_short()?;
		let year: i32 = date.get(0..4)?.parse().ok()?;
		let month: u32 = date.get(5..7)?.parse().ok()?;
		let day: u32 = date.get(8..10)?.parse().ok()?;
		let days = days_from_civil(year, month, day)?;
		Some(days * 86_400)
	}
}

/// Returns days since 1970-01-01 for a civil date.
pub(crate) fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
	if !(1..=12).contains(&month) || day == 0 || day > 31 {
		return None;
	}

	let y = year - if month <= 2 { 1 } else { 0 };
	let era = if y >= 0 { y } else { y - 399 } / 400;
	let yoe = y - era * 400;
	let m = month as i32;
	let d = day as i32;
	let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
	let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
	let days = era as i64 * 146097 + doe as i64 - 719468;
	Some(days)
}

/// Format an ISO-8601 timestamp (e.g. `2024-04-06T09:38:30.000000Z`) as `Apr 6, 2024`.
fn format_date(iso: &str) -> String {
	// Parse "YYYY-MM-DDT..."
	if iso.len() < 10 {
		return String::from(iso);
	}
	let year = &iso[0..4];
	let month = iso[5..7].parse::<u8>().unwrap_or(0);
	let day = iso[8..10].parse::<u8>().unwrap_or(0);
	let month_name = match month {
		1 => "Jan",
		2 => "Feb",
		3 => "Mar",
		4 => "Apr",
		5 => "May",
		6 => "Jun",
		7 => "Jul",
		8 => "Aug",
		9 => "Sep",
		10 => "Oct",
		11 => "Nov",
		12 => "Dec",
		_ => return String::from(iso),
	};
	format!("{} {}, {}", month_name, day, year)
}

pub(crate) fn format_size(bytes: u64) -> String {
	if bytes >= 1_073_741_824 {
		format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
	} else {
		format!("{:.1} MB", bytes as f64 / 1_048_576.0)
	}
}

impl PurchaseWork {
	/// Convert into a [`Manga`], using pre-resolved genre and series lookups.
	pub fn into_manga(
		self,
		genre_names: &BTreeMap<u32, String>,
		series_names: &BTreeMap<String, String>,
	) -> Manga {
		self.into_manga_with_date(genre_names, series_names, None)
	}

	pub fn into_manga_with_date(
		self,
		genre_names: &BTreeMap<u32, String>,
		series_names: &BTreeMap<String, String>,
		purchase_date: Option<&str>,
	) -> Manga {
		let title = self
			.name
			.as_ref()
			.map(|n| n.best())
			.unwrap_or_else(|| self.workno.clone());

		// -- Circle --
		let circle_name = self
			.maker
			.as_ref()
			.and_then(|m| m.name.as_ref())
			.map(|n| n.best());

		// -- Credits from author_name + tag classes --
		let mut author_list: Vec<String> = Vec::new();
		if let Some(ref name) = self.author_name {
			for part in name.split('/') {
				let trimmed = part.trim();
				if !trimmed.is_empty() {
					author_list.push(trimmed.into());
				}
			}
		}
		let created_by = self.tags_by_class("created_by");
		for name in &created_by {
			if !author_list.contains(name) {
				author_list.push(name.clone());
			}
		}

		let scenario_by = self.tags_by_class("scenario_by");
		let illust_by = self.tags_by_class("illust_by");
		let translated_by = self.tags_by_class("translated_by");

		// Manga.authors = author + scenario credits
		let mut authors: Vec<String> = author_list.clone();
		for name in &scenario_by {
			if !authors.contains(name) {
				authors.push(name.clone());
			}
		}
		let authors = if authors.is_empty() {
			circle_name.as_deref().map(|c| vec![c.into()])
		} else {
			Some(authors)
		};

		// Manga.artists = illustration credits
		let artists = if illust_by.is_empty() {
			None
		} else {
			Some(illust_by.clone())
		};

		// -- Tags: work type label + translated + genre_ids + tag names --
		let mut tag_list: Vec<String> = Vec::new();
		if let Some(label) = self.work_type_label() {
			tag_list.push(label.into());
		}
		if self.has_translator() {
			tag_list.push("Translated".into());
		}
		if self.is_playwork == Some(false) {
			tag_list.push("Not Playable".into());
		}
		// Resolved genre names from genre_ids
		for gid in &self.genre_ids {
			if let Some(name) = genre_names.get(gid) {
				if !tag_list.contains(name) {
					tag_list.push(name.clone());
				}
			}
		}
		// Tag names from the tags array (non-credit tags)
		if let Some(ref tags) = self.tags {
			for t in tags {
				let is_credit_tag = matches!(
					t.tag_class.as_deref(),
					Some("created_by")
						| Some("scenario_by")
						| Some("illust_by")
						| Some("translated_by")
						| Some("voice_by")
						| Some("music_by")
				);
				if !is_credit_tag && let Some(ref name) = t.name {
					if !tag_list.contains(name) {
						tag_list.push(name.clone());
					}
				}
			}
		}
		let tags = if tag_list.is_empty() {
			None
		} else {
			Some(tag_list)
		};

		// -- Description --
		let mut desc_lines: Vec<String> = Vec::new();

		if let Some(ref circle) = circle_name {
			desc_lines.push(format!("Circle: {}", circle));
		}
		if !author_list.is_empty() {
			desc_lines.push(format!("Author: {}", author_list.join(", ")));
		}
		if !scenario_by.is_empty() {
			desc_lines.push(format!("Scenario: {}", scenario_by.join(", ")));
		}
		if !illust_by.is_empty() {
			desc_lines.push(format!("Illustration: {}", illust_by.join(", ")));
		}
		if !translated_by.is_empty() {
			desc_lines.push(format!("Translation: {}", translated_by.join(", ")));
		} else if let Some(ref translator) = self.translator {
			if let Some(ref name) = translator.name {
				desc_lines.push(format!("Translator: {}", name.best()));
			}
		}
		// Series info
		if let Some(ref ws) = self.series {
			if let Some(series_name) = series_names.get(&ws.title_id) {
				let line = match ws.volume_number {
					Some(vol) => format!("Series: {} (Vol. {})", series_name, vol),
					None => format!("Series: {}", series_name),
				};
				desc_lines.push(line);
			}
		}
		// Purchase date
		if let Some(date) = purchase_date {
			desc_lines.push(format!("Purchased: {}", format_date(date)));
		}
		// File size
		if let Some(size) = self.content_size {
			if size > 0 {
				desc_lines.push(format!("Size: {}", format_size(size)));
			}
		}

		let description = if desc_lines.is_empty() {
			None
		} else {
			Some(desc_lines.join("\n"))
		};

		// -- Content rating --
		let content_rating = match self.age_category.as_deref() {
			Some("R18") | Some("r18") => ContentRating::NSFW,
			Some("R15") | Some("r15") => ContentRating::Suggestive,
			_ => ContentRating::Safe,
		};

		// -- Viewer: prefer viewer_type, fall back to work_type --
		let viewer = match self.viewer_type.as_deref() {
			Some("ebook_fixed_v2") => Viewer::RightToLeft,
			Some("play") => match self.work_type.as_deref() {
				Some("WBT") => Viewer::Webtoon,
				_ => Viewer::RightToLeft,
			},
			_ => Viewer::LeftToRight,
		};

		let url = Some(format!("https://play.dlsite.com/work/{}/tree", self.workno));

		Manga {
			key: self.workno.clone(),
			title,
			cover: self.cover_url(),
			authors,
			artists,
			description,
			tags,
			content_rating,
			viewer,
			url,
			..Default::default()
		}
	}
}

// ---------------------------------------------------------------------------
// View history from GET /api/view_histories
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct ViewHistoryEntry {
	pub workno: String,
	#[serde(default)]
	pub accessed_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Sales entry from GET /api/v3/content/sales
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct SalesEntry {
	pub workno: String,
	#[serde(default)]
	pub sales_date: Option<String>,
}

// ---------------------------------------------------------------------------
// Works response wrapper from POST /api/v3/content/works
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct SeriesInfo {
	pub id: u64,
	pub title_id: String,
	pub name: String,
	#[serde(default)]
	pub name_phonetic: Option<String>,
	#[serde(default)]
	pub total: Option<u32>,
	#[serde(default)]
	pub maker: Option<MakerInfo>,
}

#[derive(Deserialize)]
pub struct WorksResponse {
	#[serde(default)]
	pub works: Vec<PurchaseWork>,
	#[serde(default)]
	pub series: Vec<SeriesInfo>,
}

// ---------------------------------------------------------------------------
// Language editions from GET /maniax/api/=/product.json (public API)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct LanguageEdition {
	#[allow(dead_code)]
	pub workno: String,
	pub lang: String,
	pub label: String,
}

#[derive(Deserialize, Clone)]
pub struct ProductInfo {
	#[serde(default)]
	pub language_editions: Vec<LanguageEdition>,
}

// ---------------------------------------------------------------------------
// Series Manga builder
// ---------------------------------------------------------------------------

/// Returns true if a derived series name is too short or is a common
/// stop word / particle that does not constitute a meaningful title.
fn is_degenerate_name(name: &str) -> bool {
	const STOP_WORDS: &[&str] = &[
		"a", "an", "the", "in", "on", "of", "to", "at", "by", "or",
		"and", "for", "but", "not", "its", "my", "is", "it",
		"this", "that", "with", "from", "some", "than",
	];

	let lower = name.to_lowercase();
	if STOP_WORDS.contains(&lower.as_str()) {
		return true;
	}

	// CJK characters are more information-dense, so use a lower threshold.
	let first_char = name.chars().next().unwrap_or(' ');
	let is_cjk = matches!(
		first_char,
		'\u{3000}'..='\u{9FFF}'
			| '\u{F900}'..='\u{FAFF}'
			| '\u{FF00}'..='\u{FFEF}'
	);

	let min_len = if is_cjk { 2 } else { 4 };
	name.chars().count() < min_len
}

/// Derive a series name from member work titles by finding the longest common
/// prefix. Strips trailing whitespace, punctuation, and volume-like suffixes.
pub fn derive_series_name(works: &[PurchaseWork]) -> Option<String> {
	let titles: Vec<String> = works
		.iter()
		.filter_map(|w| w.name.as_ref().map(|n| n.best()))
		.collect();
	if titles.is_empty() {
		return None;
	}
	if titles.len() == 1 {
		return Some(titles.into_iter().next().unwrap());
	}

	// Find longest common prefix (byte-level, then trim to char boundary)
	let first = titles[0].as_bytes();
	let mut prefix_len = first.len();
	for t in &titles[1..] {
		let b = t.as_bytes();
		let mut i = 0;
		let limit = prefix_len.min(b.len());
		while i < limit && first[i] == b[i] {
			i += 1;
		}
		prefix_len = i;
	}

	// Trim to valid UTF-8 char boundary
	let prefix = &titles[0].as_bytes()[..prefix_len];
	let prefix = core::str::from_utf8(prefix)
		.unwrap_or_else(|e| core::str::from_utf8(&prefix[..e.valid_up_to()]).unwrap_or(""));

	// Strip trailing whitespace, punctuation, separators, and volume indicators.
	// Loop because stripping a volume suffix may reveal more punctuation.
	let mut trimmed = prefix;
	loop {
		let before = trimmed;
		trimmed = trimmed
			.trim_end()
			.trim_end_matches(|c: char| {
				matches!(c, '-' | '–' | '—' | ':' | '/' | '.' | '#' | '(' | '[' | '【')
			})
			.trim_end();

		// Strip trailing volume indicators (case-insensitive)
		for suffix in &["Volume", "Vol", "volume", "vol", "V", "v", "第"] {
			if let Some(rest) = trimmed.strip_suffix(suffix) {
				trimmed = rest;
			}
		}

		trimmed = trimmed.trim_end();
		if trimmed == before {
			break;
		}
	}

	if trimmed.is_empty() || is_degenerate_name(trimmed) {
		// No useful common prefix — fall back to first work's title
		return Some(titles.into_iter().next().unwrap());
	}

	Some(trimmed.into())
}

/// Build a single [`Manga`] entry representing a series, merging metadata from
/// all member works. `works` must be pre-sorted by volume order.
pub fn series_manga(
	title_id: &str,
	series_name: &str,
	works: &[PurchaseWork],
	genre_names: &BTreeMap<u32, String>,
	purchase_date: Option<&str>,
) -> Manga {
	let first = works.first();

	// -- Title --
	let show_prefix = crate::settings::show_series_prefix();
	let title: String = if show_prefix {
		format!("[SERIES] {}", series_name)
	} else {
		series_name.into()
	};

	// -- Cover: from the first (earliest) work --
	let cover = first.and_then(|w| w.cover_url());

	// -- Circle --
	let circle_name = first
		.and_then(|w| w.maker.as_ref())
		.and_then(|m| m.name.as_ref())
		.map(|n| n.best());

	// -- Authors / Artists: merge across works, deduplicated --
	let mut authors: Vec<String> = Vec::new();
	let mut artists: Vec<String> = Vec::new();
	for w in works {
		if let Some(ref name) = w.author_name {
			for part in name.split('/') {
				let trimmed = part.trim();
				if !trimmed.is_empty() && !authors.contains(&String::from(trimmed)) {
					authors.push(trimmed.into());
				}
			}
		}
		for name in w.tags_by_class("created_by") {
			if !authors.contains(&name) {
				authors.push(name);
			}
		}
		for name in w.tags_by_class("scenario_by") {
			if !authors.contains(&name) {
				authors.push(name);
			}
		}
		for name in w.tags_by_class("illust_by") {
			if !artists.contains(&name) {
				artists.push(name);
			}
		}
	}
	let authors = if authors.is_empty() {
		circle_name.as_deref().map(|c| vec![c.into()])
	} else {
		Some(authors)
	};
	let artists = if artists.is_empty() { None } else { Some(artists) };

	// -- Tags: merge across works --
	let mut tag_list: Vec<String> = Vec::new();
	for w in works {
		if let Some(label) = w.work_type_label() {
			let s: String = label.into();
			if !tag_list.contains(&s) {
				tag_list.push(s);
			}
		}
		if w.has_translator() && !tag_list.contains(&String::from("Translated")) {
			tag_list.push("Translated".into());
		}
		for gid in &w.genre_ids {
			if let Some(name) = genre_names.get(gid) {
				if !tag_list.contains(name) {
					tag_list.push(name.clone());
				}
			}
		}
	}
	let tags = if tag_list.is_empty() { None } else { Some(tag_list) };

	// -- Description --
	let mut desc_lines: Vec<String> = Vec::new();
	if let Some(ref circle) = circle_name {
		desc_lines.push(format!("Circle: {}", circle));
	}
	if let Some(date) = purchase_date {
		desc_lines.push(format!("Purchased: {}", format_date(date)));
	}
	desc_lines.push(format!("Volumes owned: {}", works.len()));
	for w in works {
		let name = w
			.name
			.as_ref()
			.map(|n| n.best())
			.unwrap_or_else(|| w.workno.clone());
		let vol_label = w
			.series
			.as_ref()
			.and_then(|s| s.volume_number)
			.map(|v| format!("Vol. {}", v))
			.unwrap_or_else(|| w.workno.clone());
		desc_lines.push(format!("  {} — {}", vol_label, name));
	}
	let description = Some(desc_lines.join("\n"));

	// -- Content rating: highest across works --
	let content_rating = works
		.iter()
		.map(|w| match w.age_category.as_deref() {
			Some("R18") | Some("r18") => ContentRating::NSFW,
			Some("R15") | Some("r15") => ContentRating::Suggestive,
			_ => ContentRating::Safe,
		})
		.max_by_key(|r| *r as u8)
		.unwrap_or(ContentRating::Safe);

	// -- Viewer: from first work --
	let viewer = first
		.map(|w| match w.viewer_type.as_deref() {
			Some("ebook_fixed_v2") => Viewer::RightToLeft,
			Some("play") => match w.work_type.as_deref() {
				Some("WBT") => Viewer::Webtoon,
				_ => Viewer::RightToLeft,
			},
			_ => Viewer::LeftToRight,
		})
		.unwrap_or(Viewer::RightToLeft);

	let url = Some(format!(
		"https://www.dlsite.com/{}/fsr/=/title_id/{}/order/release_d",
		crate::DLSITE_SITE_SLUG, title_id
	));

	Manga {
		key: format!("series:{}", title_id),
		title,
		cover,
		authors,
		artists,
		description,
		tags,
		content_rating,
		viewer,
		url,
		..Default::default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku::alloc::string::ToString;
	use aidoku_test::aidoku_test;

	// -- days_from_civil tests --

	#[aidoku_test]
	fn days_from_civil_unix_epoch() {
		assert_eq!(days_from_civil(1970, 1, 1), Some(0));
	}

	#[aidoku_test]
	fn days_from_civil_known_date() {
		// 2024-01-01 = 19723 days since epoch
		assert_eq!(days_from_civil(2024, 1, 1), Some(19723));
	}

	#[aidoku_test]
	fn days_from_civil_leap_year() {
		// 2024-02-29 should be valid
		assert!(days_from_civil(2024, 2, 29).is_some());
	}

	#[aidoku_test]
	fn days_from_civil_invalid_month() {
		assert_eq!(days_from_civil(2024, 0, 1), None);
		assert_eq!(days_from_civil(2024, 13, 1), None);
	}

	#[aidoku_test]
	fn days_from_civil_invalid_day() {
		assert_eq!(days_from_civil(2024, 1, 0), None);
	}

	// -- format_size tests --

	#[aidoku_test]
	fn format_size_megabytes() {
		let result = format_size(10_485_760); // 10 MB
		assert_eq!(result, "10.0 MB");
	}

	#[aidoku_test]
	fn format_size_gigabytes() {
		let result = format_size(2_147_483_648); // 2 GB
		assert_eq!(result, "2.0 GB");
	}

	#[aidoku_test]
	fn format_size_small() {
		let result = format_size(1_048_576); // 1 MB
		assert_eq!(result, "1.0 MB");
	}

	// -- derive_series_name tests --

	#[aidoku_test]
	fn derive_series_name_common_prefix_parens() {
		let works = vec![
			make_purchase_work("RJ001", Some("My Series (1)")),
			make_purchase_work("RJ002", Some("My Series (2)")),
		];
		assert_eq!(derive_series_name(&works), Some("My Series".to_string()));
	}

	#[aidoku_test]
	fn derive_series_name_strips_vol_dot() {
		let works = vec![
			make_purchase_work("RJ001", Some("My Series Vol. 1")),
			make_purchase_work("RJ002", Some("My Series Vol. 2")),
			make_purchase_work("RJ003", Some("My Series Vol. 3")),
		];
		assert_eq!(derive_series_name(&works), Some("My Series".to_string()));
	}

	#[aidoku_test]
	fn derive_series_name_strips_volume() {
		let works = vec![
			make_purchase_work("RJ001", Some("Cool Title Volume 1")),
			make_purchase_work("RJ002", Some("Cool Title Volume 2")),
		];
		assert_eq!(derive_series_name(&works), Some("Cool Title".to_string()));
	}

	#[aidoku_test]
	fn derive_series_name_strips_dash_separator() {
		let works = vec![
			make_purchase_work("RJ001", Some("My Series - 1")),
			make_purchase_work("RJ002", Some("My Series - 2")),
		];
		assert_eq!(derive_series_name(&works), Some("My Series".to_string()));
	}

	#[aidoku_test]
	fn derive_series_name_single_work() {
		let works = vec![make_purchase_work("RJ001", Some("Only Work"))];
		let result = derive_series_name(&works);
		assert_eq!(result, Some("Only Work".to_string()));
	}

	#[aidoku_test]
	fn derive_series_name_empty() {
		let works: Vec<PurchaseWork> = Vec::new();
		let result = derive_series_name(&works);
		assert_eq!(result, None);
	}

	#[aidoku_test]
	fn derive_series_name_no_common_prefix() {
		let works = vec![
			make_purchase_work("RJ001", Some("Alpha")),
			make_purchase_work("RJ002", Some("Beta")),
		];
		let result = derive_series_name(&works);
		// Falls back to first work's title
		assert_eq!(result, Some("Alpha".to_string()));
	}

	#[aidoku_test]
	fn derive_series_name_rejects_article_prefix() {
		let works = vec![
			make_purchase_work("RJ001", Some("The Great Adventure Vol. 1")),
			make_purchase_work("RJ002", Some("The Beguiling Mystery Vol. 2")),
		];
		// "The" is a stop word — falls back to first work's title
		assert_eq!(
			derive_series_name(&works),
			Some("The Great Adventure Vol. 1".to_string())
		);
	}

	#[aidoku_test]
	fn derive_series_name_rejects_short_prefix() {
		let works = vec![
			make_purchase_work("RJ001", Some("A Story Part 1")),
			make_purchase_work("RJ002", Some("A Different Story Part 2")),
		];
		// "A" is a stop word — falls back to first work's title
		assert_eq!(
			derive_series_name(&works),
			Some("A Story Part 1".to_string())
		);
	}

	#[aidoku_test]
	fn derive_series_name_accepts_long_prefix() {
		let works = vec![
			make_purchase_work("RJ001", Some("Dragon Quest Vol. 1")),
			make_purchase_work("RJ002", Some("Dragon Quest Vol. 2")),
		];
		assert_eq!(
			derive_series_name(&works),
			Some("Dragon Quest".to_string())
		);
	}

	// -- PurchaseWork helpers --

	#[aidoku_test]
	fn purchase_work_has_translator() {
		let mut w = make_purchase_work("RJ001", Some("Test"));
		assert!(!w.has_translator());
		w.translator = Some(TranslatorInfo {
			id: "t1".into(),
			name: None,
			name_phonetic: None,
		});
		assert!(w.has_translator());
	}

	#[aidoku_test]
	fn purchase_work_cover_url_protocol_relative() {
		let mut w = make_purchase_work("RJ001", Some("Test"));
		w.work_files = Some(WorkFilesInfo {
			main: Some("//img.dlsite.jp/work/123.jpg".into()),
			sam: None,
		});
		assert_eq!(w.cover_url(), Some("https://img.dlsite.jp/work/123.jpg".to_string()));
	}

	#[aidoku_test]
	fn purchase_work_cover_url_absolute() {
		let mut w = make_purchase_work("RJ001", Some("Test"));
		w.work_files = Some(WorkFilesInfo {
			main: Some("https://img.dlsite.jp/work/123.jpg".into()),
			sam: None,
		});
		assert_eq!(w.cover_url(), Some("https://img.dlsite.jp/work/123.jpg".to_string()));
	}

	#[aidoku_test]
	fn purchase_work_release_date_timestamp() {
		let mut w = make_purchase_work("RJ001", Some("Test"));
		w.regist_date = Some("2024-01-01T00:00:00".into());
		let ts = w.release_date_timestamp();
		assert_eq!(ts, Some(19723 * 86_400));
	}

	fn make_purchase_work(workno: &str, title: Option<&str>) -> PurchaseWork {
		PurchaseWork {
			workno: workno.into(),
			name: title.map(|t| LocalizedName {
				ja_JP: None,
				en_US: Some(t.into()),
				zh_CN: None,
				zh_TW: None,
				ko_KR: None,
				es_ES: None,
				de_DE: None,
				fr_FR: None,
				it_IT: None,
				pt_BR: None,
				id_ID: None,
				vi_VN: None,
				th_TH: None,
				sv_SE: None,
			}),
			name_phonetic: None,
			maker: None,
			translator: None,
			author_name: None,
			work_type: None,
			file_type: None,
			age_category: None,
			dl_format: None,
			site_id: None,
			content_length: None,
			content_count: None,
			content_size: None,
			touch_content_count: None,
			touch_site_id: None,
			os: None,
			work_files: None,
			is_playwork: None,
			downloadable: None,
			encodable: None,
			app_type: None,
			viewer_type: None,
			tags: None,
			regist_date: None,
			upgrade_date: None,
			sales_date: None,
			genre_ids: Vec::new(),
			series: None,
			purchase_type: None,
			download_start_date: None,
		}
	}
}
