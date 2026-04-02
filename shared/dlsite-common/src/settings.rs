use aidoku::{
	alloc::{String, Vec},
	imports::defaults::{DefaultValue, defaults_get, defaults_set},
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
	pub fn from_index(index: i32) -> Self {
		match index {
			1 => Self::Japanese,
			2 => Self::ChineseSimplified,
			3 => Self::ChineseTraditional,
			4 => Self::Korean,
			5 => Self::Spanish,
			6 => Self::German,
			7 => Self::French,
			8 => Self::Italian,
			9 => Self::Portuguese,
			10 => Self::Indonesian,
			11 => Self::Vietnamese,
			12 => Self::Thai,
			13 => Self::Swedish,
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
	// Aidoku stores select settings as integer indices.
	if let Some(index) = defaults_get::<i32>("preferred_language") {
		return Language::from_index(index);
	}
	Language::English
}

// ---------------------------------------------------------------------------
// Site slug
// ---------------------------------------------------------------------------

/// All-ages site slugs, indexed by the "site" select setting.
const ALL_AGES_SLUGS: &[&str] = &["home", "soft"];

/// Adult site slugs, indexed by the "site" select setting.
const ADULT_SLUGS: &[&str] = &["maniax", "pro", "books", "girls", "bl"];

/// Read the selected site from settings and return the URL slug.
/// Falls back to `default_slug` if no setting is found.
pub fn get_site_slug(default_slug: &str) -> &'static str {
	let index = defaults_get::<i32>("site").unwrap_or(0) as usize;
	// Use the default slug to determine which site list applies.
	let slugs = if ADULT_SLUGS.contains(&default_slug) {
		ADULT_SLUGS
	} else {
		ALL_AGES_SLUGS
	};
	slugs.get(index).copied().unwrap_or(slugs[0])
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
// Login / web cookies
// ---------------------------------------------------------------------------

const LOGGED_IN_KEY: &str = "logged_in";
const WEB_COOKIES_KEY: &str = "web_cookies";

pub fn is_logged_in() -> bool {
	defaults_get::<bool>(LOGGED_IN_KEY).unwrap_or(false)
}

pub fn set_logged_in(value: bool) {
	defaults_set(LOGGED_IN_KEY, DefaultValue::Bool(value));
}

/// Full `Cookie` header value populated from web login.
pub fn set_web_cookies(header_value: &str) {
	defaults_set(
		WEB_COOKIES_KEY,
		DefaultValue::String(String::from(header_value)),
	);
}

pub fn get_web_cookies() -> Option<String> {
	defaults_get::<String>(WEB_COOKIES_KEY).filter(|s| !s.is_empty())
}

pub fn clear_web_cookies() {
	defaults_set(WEB_COOKIES_KEY, DefaultValue::Null);
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
