use aidoku::{
	alloc::{String, Vec},
	imports::defaults::defaults_get,
};

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
	English = 0,
	Japanese = 1,
	ChineseSimplified = 2,
	ChineseTraditional = 3,
	Korean = 4,
	Spanish = 5,
	German = 6,
	French = 7,
	Italian = 8,
	Portuguese = 9,
	Indonesian = 10,
	Vietnamese = 11,
	Thai = 12,
	Swedish = 13,
}

impl Language {
	pub fn from_setting(s: Option<&str>) -> Self {
		match s {
			Some("Japanese") => Self::Japanese,
			Some("Chinese (Simplified)") => Self::ChineseSimplified,
			Some("Chinese (Traditional)") => Self::ChineseTraditional,
			Some("Korean") => Self::Korean,
			Some("Spanish") => Self::Spanish,
			Some("German") => Self::German,
			Some("French") => Self::French,
			Some("Italian") => Self::Italian,
			Some("Portuguese (Brazil)") => Self::Portuguese,
			Some("Indonesian") => Self::Indonesian,
			Some("Vietnamese") => Self::Vietnamese,
			Some("Thai") => Self::Thai,
			Some("Swedish") => Self::Swedish,
			_ => Self::English,
		}
	}

	pub fn locale_code(self) -> &'static str {
		match self {
			Self::English => "en_US",
			Self::Japanese => "ja_JP",
			Self::ChineseSimplified => "zh_CN",
			Self::ChineseTraditional => "zh_TW",
			Self::Korean => "ko_KR",
			Self::Spanish => "es_ES",
			Self::German => "de_DE",
			Self::French => "fr_FR",
			Self::Italian => "it_IT",
			Self::Portuguese => "pt_BR",
			Self::Indonesian => "id_ID",
			Self::Vietnamese => "vi_VN",
			Self::Thai => "th_TH",
			Self::Swedish => "sv_SE",
		}
	}
}

pub fn get_preferred_language() -> Language {
	Language::from_setting(defaults_get::<String>("preferred_language").as_deref())
}

// ---------------------------------------------------------------------------
// Site slug
// ---------------------------------------------------------------------------

/// Read the selected site from settings and return the URL slug.
/// Falls back to `default_slug` if no setting is found.
pub fn get_site_slug(default_slug: &str) -> &str {
	let setting = defaults_get::<String>("site");
	match setting.as_deref() {
		// All-ages sites
		Some("Doujin") => "home",
		Some("PC Games") => "soft",
		// Adult sites
		Some("Adult Doujin") => "maniax",
		Some("H Games") => "pro",
		Some("Adult Comics") => "books",
		Some("Otome") => "girls",
		Some("BL") => "bl",
		_ => default_slug,
	}
}

// ---------------------------------------------------------------------------
// Content rating
// ---------------------------------------------------------------------------

/// Read default content rating filter from multi-select setting.
/// Returns the list of selected rating codes (e.g. `["safe", "r15"]`).
/// Empty vec means "all" (no filtering).
pub fn get_default_content_ratings() -> Vec<String> {
	defaults_get::<Vec<String>>("default_content_rating").unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Work type setting
// ---------------------------------------------------------------------------

/// Read work type filter from settings multi-selects.
/// Returns the list of enabled work type codes.
pub fn get_work_type_setting() -> Vec<String> {
	let keys = [
		"wt_images", "wt_av", "wt_game", "wt_tools", "wt_misc",
	];
	let mut selected = Vec::new();
	for key in &keys {
		if let Some(values) = defaults_get::<Vec<String>>(key) {
			selected.extend(values);
		}
	}
	selected
}
