use aidoku::{
	alloc::{String, Vec},
	imports::defaults::{DefaultValue, defaults_get, defaults_set},
};

const CACHED_WORKNOS_KEY: &str = "cached_worknos";
const LOGGED_IN_KEY: &str = "logged_in";
const LOGIN_USERNAME_KEY: &str = "login_username";
const LOGIN_PASSWORD_KEY: &str = "login_password";

pub fn is_logged_in() -> bool {
	defaults_get::<bool>(LOGGED_IN_KEY).unwrap_or(false)
}

pub fn set_logged_in(value: bool) {
	defaults_set(LOGGED_IN_KEY, DefaultValue::Bool(value));
}

pub fn set_credentials(username: &str, password: &str) {
	defaults_set(
		LOGIN_USERNAME_KEY,
		DefaultValue::String(String::from(username)),
	);
	defaults_set(
		LOGIN_PASSWORD_KEY,
		DefaultValue::String(String::from(password)),
	);
}

pub fn get_credentials() -> Option<(String, String)> {
	let username = defaults_get::<String>(LOGIN_USERNAME_KEY).unwrap_or_default();
	let password = defaults_get::<String>(LOGIN_PASSWORD_KEY).unwrap_or_default();
	if username.is_empty() || password.is_empty() {
		None
	} else {
		Some((username, password))
	}
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
