use aidoku::{
	ContentRating, Manga, Viewer,
	alloc::{String, Vec, collections::BTreeMap, format, vec},
	serde::Deserialize,
};

// ---------------------------------------------------------------------------
// Download token from GET /api/v3/download/sign/cookie
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct DownloadToken {
	#[allow(dead_code)]
	pub expires: String,
	pub url: String,
}

// ---------------------------------------------------------------------------
// PlayFile from ziptree.json playfile entries
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct OptimizedInfo {
	pub name: Option<String>,
	#[allow(dead_code)]
	pub length: Option<i64>,
	pub width: Option<i32>,
	pub height: Option<i32>,
	pub crypt: Option<bool>,
}

#[derive(Deserialize, Clone)]
pub struct PdfPageOptimized {
	pub name: Option<String>,
	pub length: Option<i64>,
	pub width: Option<i32>,
	pub height: Option<i32>,
	pub crypt: Option<bool>,
}

#[derive(Deserialize, Clone)]
pub struct PdfPage {
	pub optimized: Option<PdfPageOptimized>,
}

#[derive(Deserialize, Clone)]
pub struct PlayFileFiles {
	pub optimized: Option<OptimizedInfo>,
	pub page: Option<Vec<PdfPage>>,
}

#[derive(Clone)]
pub struct PlayFile {
	#[allow(dead_code)]
	pub length: i64,
	pub file_type: String,
	pub files: PlayFileFiles,
	#[allow(dead_code)]
	pub hashname: String,
}

impl PlayFile {
	pub fn optimized_name(&self) -> Option<&str> {
		self.files
			.optimized
			.as_ref()
			.and_then(|o| o.name.as_deref())
	}

	pub fn is_crypt(&self) -> bool {
		self.files
			.optimized
			.as_ref()
			.and_then(|o| o.crypt)
			.unwrap_or(false)
	}

	pub fn crypt_dimensions(&self) -> Option<(i32, i32)> {
		let opt = self.files.optimized.as_ref()?;
		Some((opt.width?, opt.height?))
	}
}

/// Raw JSON shape for a playfile entry inside ziptree.json.
/// The `type` field is a reserved keyword, so we deserialize into this
/// intermediate struct and then convert to `PlayFile`.
#[derive(Deserialize)]
pub struct RawPlayFile {
	pub length: i64,
	#[serde(rename = "type")]
	pub file_type: String,
	#[serde(default)]
	pub image: Option<PlayFileFiles>,
	#[serde(default)]
	pub pdf: Option<PlayFileFiles>,
	#[serde(default)]
	pub video: Option<PlayFileFiles>,
	#[serde(default)]
	pub ebook_fixed: Option<PlayFileFiles>,
	#[serde(default)]
	pub epub: Option<PlayFileFiles>,
	#[serde(default)]
	pub epub_reflowable: Option<PlayFileFiles>,
	#[serde(default)]
	pub voicecomic_v2: Option<PlayFileFiles>,
}

impl RawPlayFile {
	pub fn into_playfile(self, hashname: String) -> PlayFile {
		let files = match self.file_type.as_str() {
			"image" => self.image,
			"pdf" => self.pdf,
			"video" => self.video,
			"ebook_fixed" => self.ebook_fixed,
			"epub" => self.epub,
			"epub_reflowable" => self.epub_reflowable,
			"voicecomic_v2" => self.voicecomic_v2,
			_ => None,
		};
		PlayFile {
			length: self.length,
			file_type: self.file_type,
			files: files.unwrap_or(PlayFileFiles {
				optimized: None,
				page: None,
			}),
			hashname,
		}
	}
}

// ---------------------------------------------------------------------------
// ZipTree from GET {token.url}ziptree.json
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RawTreeEntry {
	#[serde(rename = "type")]
	pub entry_type: String,
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub path: Option<String>,
	#[serde(default)]
	pub hashname: Option<String>,
	#[serde(default)]
	pub children: Option<Vec<RawTreeEntry>>,
}

#[derive(Deserialize)]
pub struct RawZipTree {
	pub hash: String,
	#[serde(default)]
	pub playfile: BTreeMap<String, RawPlayFile>,
	#[serde(default)]
	pub tree: Vec<RawTreeEntry>,
}

pub struct ZipTree {
	#[allow(dead_code)]
	pub hash: String,
	pub playfiles: BTreeMap<String, PlayFile>,
	pub tree: Vec<RawTreeEntry>,
}

impl ZipTree {
	pub fn from_raw(raw: RawZipTree) -> Self {
		let playfiles: BTreeMap<String, PlayFile> = raw
			.playfile
			.into_iter()
			.map(|(k, v): (String, RawPlayFile)| {
				let pf = v.into_playfile(k.clone());
				(k, pf)
			})
			.collect();
		ZipTree {
			hash: raw.hash,
			playfiles,
			tree: raw.tree,
		}
	}

	/// Walk the tree and return `(relative_path, PlayFile)` pairs.
	pub fn walk(&self) -> Vec<(String, PlayFile)> {
		let mut result = Vec::new();
		walk_entries(&self.tree, None, &self.playfiles, &mut result);
		result
	}
}

fn walk_entries(
	entries: &[RawTreeEntry],
	parent: Option<&str>,
	playfiles: &BTreeMap<String, PlayFile>,
	out: &mut Vec<(String, PlayFile)>,
) {
	for entry in entries {
		match entry.entry_type.as_str() {
			"file" => {
				if let (Some(name), Some(hashname)) = (&entry.name, &entry.hashname) {
					let path: String = match parent {
						Some(p) => format!("{}/{}", p, name),
						None => name.clone(),
					};
					if let Some(pf) = playfiles.get(hashname.as_str()) {
						out.push((path, (*pf).clone()));
					}
				}
			}
			"folder" => {
				let folder_path = entry.path.as_deref().or(entry.name.as_deref());
				if let Some(children) = &entry.children {
					walk_entries(children, folder_path, playfiles, out);
				}
			}
			_ => {}
		}
	}
}

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
}

impl LocalizedName {
	pub fn best(&self) -> String {
		self.ja_JP
			.as_deref()
			.or(self.en_US.as_deref())
			.or(self.zh_CN.as_deref())
			.or(self.zh_TW.as_deref())
			.or(self.ko_KR.as_deref())
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
fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
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

fn format_size(bytes: u64) -> String {
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
// Sales entry from GET /api/v3/content/sales
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct SalesEntry {
	pub workno: String,
	#[serde(default)]
	#[allow(dead_code)]
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

/// Build a single [`Manga`] entry representing a series, merging metadata from
/// all member works. `works` must be pre-sorted by volume order.
pub fn series_manga(
	title_id: &str,
	series_name: &str,
	works: &[PurchaseWork],
	genre_names: &BTreeMap<u32, String>,
) -> Manga {
	let first = works.first();

	// -- Title --
	let title = series_name.into();

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
		"https://www.dlsite.com/maniax/fsr/=/title_id/{}/order/release_d",
		title_id
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
