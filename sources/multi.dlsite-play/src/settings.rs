use aidoku::{
	alloc::{String, Vec},
	imports::defaults::{DefaultValue, defaults_get, defaults_set},
};

const CACHED_WORKNOS_KEY: &str = "cached_worknos";
const LOGGED_IN_KEY: &str = "logged_in";
const WEB_COOKIES_KEY: &str = "web_cookies";
const USERNAME_KEY: &str = "username";
const PASSWORD_KEY: &str = "password";

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
}

pub fn clear_cached_page() {
	defaults_set("cached_page", DefaultValue::Null);
}

/// Full `Cookie` header value for `play.dlsite.com` / `play.dl.dlsite.com` requests.
/// Populated from direct credential login; Aidoku does not share WebView cookies with WASM `Request`s.
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

pub fn get_credentials() -> Option<(String, String)> {
	let username = defaults_get::<String>(USERNAME_KEY).filter(|s| !s.is_empty())?;
	let password = defaults_get::<String>(PASSWORD_KEY).filter(|s| !s.is_empty())?;
	Some((username, password))
}
