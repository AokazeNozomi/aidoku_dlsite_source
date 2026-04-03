use aidoku::{
	alloc::{collections::BTreeMap, format, String, Vec},
	imports::defaults::{DefaultValue, defaults_get, defaults_set},
};

// ---------------------------------------------------------------------------
// Enums for select settings
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortOption {
	RecentlyOpened = 0,
	PurchaseDate = 1,
	ReleaseDate = 2,
	WriterCircle = 3,
	Title = 4,
}

impl SortOption {
	pub fn from_index(index: i32) -> Self {
		match index {
			1 => Self::PurchaseDate,
			2 => Self::ReleaseDate,
			3 => Self::WriterCircle,
			4 => Self::Title,
			_ => Self::RecentlyOpened,
		}
	}

}


// ---------------------------------------------------------------------------
// Settings getters
// ---------------------------------------------------------------------------

const PREFERRED_LANGUAGE_KEY: &str = "preferred_language";
const SERIES_PREFIX_KEY: &str = "series_prefix";
const CACHED_WORKNOS_KEY: &str = "cached_worknos";
const SALES_FETCHED_AT_KEY: &str = "sales_fetched_at_unix";

pub fn get_preferred_language() -> Language {
	match defaults_get::<String>(PREFERRED_LANGUAGE_KEY).as_deref() {
		Some("Japanese") => Language::Japanese,
		Some("Chinese (Simplified)") => Language::ChineseSimplified,
		Some("Chinese (Traditional)") => Language::ChineseTraditional,
		Some("Korean") => Language::Korean,
		Some("Spanish") => Language::Spanish,
		Some("German") => Language::German,
		Some("French") => Language::French,
		Some("Italian") => Language::Italian,
		Some("Portuguese (Brazil)") => Language::Portuguese,
		Some("Indonesian") => Language::Indonesian,
		Some("Vietnamese") => Language::Vietnamese,
		Some("Thai") => Language::Thai,
		Some("Swedish") => Language::Swedish,
		_ => Language::English,
	}
}

pub fn show_series_prefix() -> bool {
	defaults_get::<bool>(SERIES_PREFIX_KEY).unwrap_or(false)
}

pub use dlsite_common::settings::{
	is_logged_in, set_logged_in, get_web_cookies, set_web_cookies, clear_web_cookies,
};

/// Store the full list of purchased work IDs for pagination.
pub fn set_cached_worknos(worknos: &[String]) {
	let joined: String = worknos.join(",");
	defaults_set(CACHED_WORKNOS_KEY, DefaultValue::String(joined));
}

/// Retrieve the cached list of purchased work IDs.
pub fn get_cached_worknos() -> Vec<String> {
	defaults_get::<String>(CACHED_WORKNOS_KEY)
		.filter(|s| !s.is_empty())
		.map(|s| s.split(',').map(|w| w.into()).collect())
		.unwrap_or_default()
}

pub fn clear_cached_worknos() {
	defaults_set(CACHED_WORKNOS_KEY, DefaultValue::Null);
	defaults_set(CACHED_SALES_DATES_KEY, DefaultValue::Null);
	defaults_set(SALES_FETCHED_AT_KEY, DefaultValue::Null);
	clear_cached_series_maps();
}

const CACHED_SALES_DATES_KEY: &str = "cached_sales_dates";

/// Store sales dates as `"workno=date,workno=date,..."`.
pub fn set_cached_sales_dates(dates: &BTreeMap<String, String>) {
	let joined: String = dates
		.iter()
		.map(|(k, v)| format!("{}={}", k, v))
		.collect::<Vec<_>>()
		.join(",");
	defaults_set(CACHED_SALES_DATES_KEY, DefaultValue::String(joined));
}

/// Retrieve cached sales dates as workno → sales_date map.
pub fn get_cached_sales_dates() -> BTreeMap<String, String> {
	defaults_get::<String>(CACHED_SALES_DATES_KEY)
		.filter(|s| !s.is_empty())
		.map(|s| {
			s.split(',')
				.filter_map(|pair| {
					let mut parts = pair.splitn(2, '=');
					let k = parts.next()?;
					let v = parts.next()?;
					Some((String::from(k), String::from(v)))
				})
				.collect()
		})
		.unwrap_or_default()
}

/// Unix time when `/content/sales` last succeeded and populated [Self::get_cached_worknos].
pub fn get_sales_fetched_at() -> Option<i64> {
	defaults_get::<String>(SALES_FETCHED_AT_KEY).and_then(|s| s.parse().ok())
}

pub fn set_sales_fetched_at(ts: i64) {
	defaults_set(
		SALES_FETCHED_AT_KEY,
		DefaultValue::String(format!("{}", ts)),
	);
}

pub fn clear_cached_page() {
	defaults_set("cached_page", DefaultValue::Null);
}


const CACHED_GENRES_KEY: &str = "cached_genres";
const CACHED_GENRES_LANG_KEY: &str = "cached_genres_lang";

/// Store resolved genre ID→name pairs as `"id:name\nid:name\n..."`.
pub fn set_cached_genres(value: &str) {
	defaults_set(
		CACHED_GENRES_KEY,
		DefaultValue::String(String::from(value)),
	);
	defaults_set(
		CACHED_GENRES_LANG_KEY,
		DefaultValue::String(format!("{}", get_preferred_language() as i32)),
	);
}

/// Retrieve cached genre ID→name pairs.
/// Returns `None` if no cache exists or if the language setting has changed.
pub fn get_cached_genres() -> Option<String> {
	let cached_lang = defaults_get::<String>(CACHED_GENRES_LANG_KEY)
		.and_then(|s| s.parse::<i32>().ok())
		.unwrap_or(-1);
	if cached_lang != get_preferred_language() as i32 {
		return None;
	}
	defaults_get::<String>(CACHED_GENRES_KEY).filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// Series mapping cache: title_id → worknos
// ---------------------------------------------------------------------------

fn series_map_key(title_id: &str) -> String {
	format!("series_map_{}", title_id)
}

const SERIES_TITLE_IDS_KEY: &str = "series_title_ids";

/// Store the worknos belonging to a series.
pub fn set_cached_series_map(title_id: &str, worknos: &[String]) {
	let joined: String = worknos.join(",");
	defaults_set(
		&series_map_key(title_id),
		DefaultValue::String(joined),
	);
}

/// Retrieve cached worknos for a series.
pub fn get_cached_series_map(title_id: &str) -> Vec<String> {
	defaults_get::<String>(&series_map_key(title_id))
		.filter(|s| !s.is_empty())
		.map(|s| s.split(',').map(|w| w.into()).collect())
		.unwrap_or_default()
}

/// Store the list of known series title_ids (for bulk clearing).
pub fn set_cached_series_title_ids(title_ids: &[String]) {
	let joined: String = title_ids.join(",");
	defaults_set(SERIES_TITLE_IDS_KEY, DefaultValue::String(joined));
}

/// Clear all cached series mappings.
pub fn clear_cached_series_maps() {
	let title_ids: Vec<String> = defaults_get::<String>(SERIES_TITLE_IDS_KEY)
		.filter(|s| !s.is_empty())
		.map(|s| s.split(',').map(|w| w.into()).collect())
		.unwrap_or_default();
	for tid in &title_ids {
		defaults_set(&series_map_key(tid), DefaultValue::Null);
	}
	defaults_set(SERIES_TITLE_IDS_KEY, DefaultValue::Null);
}

// ---------------------------------------------------------------------------
// Language cache
// ---------------------------------------------------------------------------

fn lang_cache_key(workno: &str) -> String {
	format!("lang_{}", workno)
}

pub fn get_cached_languages(workno: &str) -> Option<String> {
	defaults_get::<String>(&lang_cache_key(workno)).filter(|s| !s.is_empty())
}

pub fn set_cached_languages(workno: &str, value: &str) {
	defaults_set(
		&lang_cache_key(workno),
		DefaultValue::String(String::from(value)),
	);
}

// ---------------------------------------------------------------------------
// Language filter (Source Settings > Languages)
// ---------------------------------------------------------------------------

/// Read the user's selected languages from Aidoku's Source Settings and
/// return them as DLsite API code strings (e.g. `["JPN", "ENG"]`).
/// Returns an empty Vec when all languages are selected (no filtering).
pub fn get_selected_languages() -> Vec<String> {
	dlsite_common::settings::get_selected_languages()
		.iter()
		.map(|l| String::from(l.api_code()))
		.collect()
}

/// Check whether a work's cached language data matches any of the selected
/// DLsite language codes. Returns `true` if:
/// - `dlsite_codes` is empty (no filter active), OR
/// - the work has no cached language data (unknown = keep), OR
/// - at least one of the work's languages appears in `dlsite_codes`.
pub fn work_matches_languages(workno: &str, dlsite_codes: &[String]) -> bool {
	if dlsite_codes.is_empty() {
		return true;
	}
	let cached = match get_cached_languages(workno) {
		Some(c) => c,
		None => return true, // unknown language → don't filter out
	};
	// Cache format: "JPN:Japanese,ENG:English"
	for pair in cached.split(',') {
		if let Some(code) = pair.split(':').next() {
			if dlsite_codes.iter().any(|c| c == code) {
				return true;
			}
		}
	}
	false
}

pub use dlsite_common::settings::get_work_type_setting;

// ---------------------------------------------------------------------------
// View history throttle cache
// ---------------------------------------------------------------------------

const LAST_VIEWED_KEY: &str = "last_viewed_workno";

pub fn get_last_viewed_workno() -> Option<String> {
	defaults_get::<String>(LAST_VIEWED_KEY).filter(|s| !s.is_empty())
}

pub fn set_last_viewed_workno(workno: &str) {
	defaults_set(
		LAST_VIEWED_KEY,
		DefaultValue::String(String::from(workno)),
	);
}

pub fn update_view_history_enabled() -> bool {
	defaults_get::<bool>("update_view_history").unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Sort settings
// ---------------------------------------------------------------------------

pub fn get_default_sort() -> SortOption {
	match defaults_get::<String>("default_sort").as_deref() {
		Some("Recently Opened") => SortOption::RecentlyOpened,
		Some("Release Date") => SortOption::ReleaseDate,
		Some("Writer/Circle Name") => SortOption::WriterCircle,
		Some("Title") => SortOption::Title,
		_ => SortOption::PurchaseDate,
	}
}

pub fn get_default_sort_ascending() -> bool {
	defaults_get::<String>("default_sort_ascending").as_deref() == Some("Ascending")
}

pub use dlsite_common::settings::get_default_content_ratings;

