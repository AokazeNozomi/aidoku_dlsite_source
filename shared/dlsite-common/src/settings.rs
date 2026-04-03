use aidoku::{
	alloc::{format, String, Vec},
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

	/// DLsite locale cookie value (e.g. `"en-us"`), used to localize
	/// work titles in FSR search/listing responses.
	pub fn locale_cookie_code(self) -> &'static str {
		match self {
			Self::Japanese => "ja-jp",
			Self::English => "en-us",
			Self::ChineseSimplified => "zh-cn",
			Self::ChineseTraditional => "zh-tw",
			Self::Korean => "ko-kr",
			Self::Spanish => "es-es",
			Self::German => "de-de",
			Self::French => "fr-fr",
			Self::Indonesian => "id-id",
			Self::Italian => "it-it",
			Self::Portuguese => "pt-br",
			Self::Swedish => "sv-se",
			Self::Thai => "th-th",
			Self::Vietnamese => "vi-vn",
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

/// Build a `Cookie` header value that includes the DLsite locale cookie
/// (for localized work titles) and any existing web login cookies.
pub fn get_locale_cookie_header() -> String {
	let locale = get_preferred_language().locale_cookie_code();
	match get_web_cookies() {
		Some(cookies) => format!("locale={}; {}", locale, cookies),
		None => format!("locale={}", locale),
	}
}

// ---------------------------------------------------------------------------
// DLsite language codes
// ---------------------------------------------------------------------------

/// DLsite API language codes used in FSR search URLs and filter options.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DlsiteLang {
	JPN,
	ENG,
	ChiHans,
	ChiHant,
	KoKr,
	SPA,
	GER,
	FRE,
	IND,
	ITA,
	POR,
	SWE,
	THA,
	VIE,
}

impl DlsiteLang {
	/// The API code string used in FSR URL paths and filter values.
	pub fn api_code(self) -> &'static str {
		match self {
			Self::JPN => "JPN",
			Self::ENG => "ENG",
			Self::ChiHans => "CHI_HANS",
			Self::ChiHant => "CHI_HANT",
			Self::KoKr => "KO_KR",
			Self::SPA => "SPA",
			Self::GER => "GER",
			Self::FRE => "FRE",
			Self::IND => "IND",
			Self::ITA => "ITA",
			Self::POR => "POR",
			Self::SWE => "SWE",
			Self::THA => "THA",
			Self::VIE => "VIE",
		}
	}

	/// Parse from the API code string (e.g. `"JPN"`, `"ENG"`).
	pub fn from_api_code(code: &str) -> Option<Self> {
		match code {
			"JPN" => Some(Self::JPN),
			"ENG" => Some(Self::ENG),
			"CHI_HANS" => Some(Self::ChiHans),
			"CHI_HANT" => Some(Self::ChiHant),
			"KO_KR" => Some(Self::KoKr),
			"SPA" => Some(Self::SPA),
			"GER" => Some(Self::GER),
			"FRE" => Some(Self::FRE),
			"IND" => Some(Self::IND),
			"ITA" => Some(Self::ITA),
			"POR" => Some(Self::POR),
			"SWE" => Some(Self::SWE),
			"THA" => Some(Self::THA),
			"VIE" => Some(Self::VIE),
			_ => None,
		}
	}

	/// Parse from Aidoku source.json language code (e.g. `"ja"`, `"en"`).
	pub fn from_source_code(code: &str) -> Option<Self> {
		match code {
			"ja" => Some(Self::JPN),
			"en" => Some(Self::ENG),
			"zh-Hans" => Some(Self::ChiHans),
			"zh-Hant" => Some(Self::ChiHant),
			"ko" => Some(Self::KoKr),
			"es" => Some(Self::SPA),
			"de" => Some(Self::GER),
			"fr" => Some(Self::FRE),
			"id" => Some(Self::IND),
			"it" => Some(Self::ITA),
			"pt" => Some(Self::POR),
			"sv" => Some(Self::SWE),
			"th" => Some(Self::THA),
			"vi" => Some(Self::VIE),
			_ => None,
		}
	}

	/// English display name for this language.
	pub fn english_name(self) -> &'static str {
		match self {
			Self::JPN => "Japanese",
			Self::ENG => "English",
			Self::ChiHans => "Chinese (Simplified)",
			Self::ChiHant => "Chinese (Traditional)",
			Self::KoKr => "Korean",
			Self::SPA => "Spanish",
			Self::GER => "German",
			Self::FRE => "French",
			Self::IND => "Indonesian",
			Self::ITA => "Italian",
			Self::POR => "Portuguese",
			Self::SWE => "Swedish",
			Self::THA => "Thai",
			Self::VIE => "Vietnamese",
		}
	}
}

// ---------------------------------------------------------------------------
// Source language filter
// ---------------------------------------------------------------------------

/// Read the user's selected languages from Aidoku's Source Settings and
/// return them as `DlsiteLang` values.
/// Returns an empty Vec when all languages are selected (no filtering).
pub fn get_selected_languages() -> Vec<DlsiteLang> {
	let codes: Vec<String> = defaults_get::<Vec<String>>("languages").unwrap_or_default();
	if codes.is_empty() {
		return Vec::new();
	}
	// If every defined language is selected, treat it as "no filter".
	// source.json defines 14 languages.
	if codes.len() >= 14 {
		return Vec::new();
	}
	codes
		.iter()
		.filter_map(|c| DlsiteLang::from_source_code(c))
		.collect()
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
