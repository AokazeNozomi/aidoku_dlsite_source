use aidoku::{
	alloc::{format, String, Vec},
	imports::defaults::{DefaultValue, defaults_get, defaults_set},
};

const CACHED_WORKNOS_KEY: &str = "cached_worknos";
const LOGGED_IN_KEY: &str = "logged_in";
const WEB_COOKIES_KEY: &str = "web_cookies";
const SALES_FETCHED_AT_KEY: &str = "sales_fetched_at_unix";

pub fn is_logged_in() -> bool {
	defaults_get::<bool>(LOGGED_IN_KEY).unwrap_or(false)
}

pub fn set_logged_in(value: bool) {
	defaults_set(LOGGED_IN_KEY, DefaultValue::Bool(value));
}

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
	defaults_set(SALES_FETCHED_AT_KEY, DefaultValue::Null);
	clear_cached_series_maps();
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

/// Full `Cookie` header value for `play.dlsite.com` / `play.dl.dlsite.com` requests.
/// Populated from web login; Aidoku does not attach WebView cookies to WASM `Request`s.
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

const CACHED_GENRES_KEY: &str = "cached_genres";

/// Store resolved genre ID→name pairs as `"id:name,id:name,..."`.
pub fn set_cached_genres(value: &str) {
	defaults_set(
		CACHED_GENRES_KEY,
		DefaultValue::String(String::from(value)),
	);
}

/// Retrieve cached genre ID→name pairs.
/// Returns empty string if no cache exists.
pub fn get_cached_genres() -> Option<String> {
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
