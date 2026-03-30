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
pub struct MakerInfo {
	pub id: String,
	#[serde(default)]
	pub name: Option<LocalizedName>,
}

#[derive(Deserialize, Clone)]
pub struct WorkFilesInfo {
	#[serde(default)]
	pub main: Option<String>,
	#[serde(default)]
	pub sam: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct WorkTag {
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default, rename = "class")]
	pub tag_class: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct PurchaseWork {
	pub workno: String,
	#[serde(default)]
	pub name: Option<LocalizedName>,
	#[serde(default)]
	pub maker: Option<MakerInfo>,
	#[serde(default)]
	pub age_category: Option<String>,
	#[serde(default)]
	pub work_type: Option<String>,
	#[serde(default)]
	pub work_files: Option<WorkFilesInfo>,
	#[serde(default)]
	pub author_name: Option<String>,
	#[serde(default)]
	pub regist_date: Option<String>,
	#[serde(default)]
	pub upgrade_date: Option<String>,
	#[serde(default)]
	pub tags: Option<Vec<WorkTag>>,
	#[serde(default)]
	pub sales_date: Option<String>,
	#[serde(default)]
	pub language: Option<Vec<String>>,
}

impl PurchaseWork {
	fn normalize_language_code(lang: &str) -> Option<&'static str> {
		let trimmed = lang.trim();
		let lower = trimmed.to_lowercase();
		match lower.as_str() {
			"ja" | "ja_jp" | "ja-jp" | "japanese" | "日本語" => Some("ja"),
			"en" | "en_us" | "en-us" | "english" | "英語" => Some("en"),
			"zh_cn" | "zh-cn" | "zh-hans" | "简体中文" | "簡体中文" => Some("zh-Hans"),
			"zh_tw" | "zh-tw" | "zh-hant" | "繁體中文" | "繁体中文" => Some("zh-Hant"),
			"ko" | "ko_kr" | "ko-kr" | "korean" | "한국어" => Some("ko"),
			"es" | "spanish" | "español" => Some("es"),
			"ar" | "arabic" | "العربية" => Some("ar"),
			"de" | "german" | "deutsch" => Some("de"),
			"fr" | "french" | "français" => Some("fr"),
			"id" | "indonesian" | "bahasa indonesia" => Some("id"),
			"it" | "italian" | "italiano" => Some("it"),
			"pt" | "portuguese" | "português" => Some("pt"),
			"sv" | "swedish" | "svenska" => Some("sv"),
			"th" | "thai" | "ไทย" => Some("th"),
			"vi" | "vietnamese" | "tiếng việt" => Some("vi"),
			_ => None,
		}
	}

	pub fn primary_language_code(&self) -> Option<String> {
		let langs = self.language.as_ref()?;
		for lang in langs {
			if let Some(code) = Self::normalize_language_code(lang) {
				return Some(code.into());
			}
		}
		None
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
}

impl From<PurchaseWork> for Manga {
	fn from(work: PurchaseWork) -> Self {
		let title = work
			.name
			.as_ref()
			.map(|n| n.best())
			.unwrap_or_else(|| work.workno.clone());

		// -- Circle --
		let circle_name = work
			.maker
			.as_ref()
			.and_then(|m| m.name.as_ref())
			.map(|n| n.best());

		// -- Credits from author_name + tag classes --
		let mut author_list: Vec<String> = Vec::new();
		if let Some(ref name) = work.author_name {
			for part in name.split('/') {
				let trimmed = part.trim();
				if !trimmed.is_empty() {
					author_list.push(trimmed.into());
				}
			}
		}
		let created_by = work.tags_by_class("created_by");
		for name in &created_by {
			if !author_list.contains(name) {
				author_list.push(name.clone());
			}
		}

		let scenario_by = work.tags_by_class("scenario_by");
		let illust_by = work.tags_by_class("illust_by");
		let translated_by = work.tags_by_class("translated_by");

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

		// -- Tags: genre tags + work type label --
		let mut tag_list: Vec<String> = Vec::new();
		if let Some(label) = work.work_type_label() {
			tag_list.push(label.into());
		}
		if let Some(ref tags) = work.tags {
			for t in tags {
				let dominated = t.tag_class.is_none()
					|| t.tag_class.as_deref() == Some("")
					|| t.tag_class.as_deref() == Some("genre");
				if dominated {
					if let Some(ref name) = t.name {
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
		}
		if let Some(date) = work.release_date_short() {
			desc_lines.push(format!("Release Date: {}", date));
		}
		if let Some(label) = work.work_type_label() {
			desc_lines.push(format!("Type: {}", label));
		}
		if let Some(ref langs) = work.language {
			if !langs.is_empty() {
				desc_lines.push(format!("Language: {}", langs.join(", ")));
			}
		}

		let description = if desc_lines.is_empty() {
			None
		} else {
			Some(desc_lines.join("\n"))
		};

		// -- Content rating & viewer --
		let content_rating = match work.age_category.as_deref() {
			Some("R18") | Some("r18") => ContentRating::NSFW,
			Some("R15") | Some("r15") => ContentRating::Suggestive,
			_ => ContentRating::Safe,
		};

		let viewer = match work.work_type.as_deref() {
			Some("MNG") | Some("WBT") => Viewer::RightToLeft,
			_ => Viewer::LeftToRight,
		};

		let url = Some(format!("https://play.dlsite.com/#/work/{}", work.workno));

		Manga {
			key: work.workno.clone(),
			title,
			cover: work.cover_url(),
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

#[derive(Deserialize)]
pub struct WorksResponse {
	#[serde(default)]
	pub works: Vec<PurchaseWork>,
}
