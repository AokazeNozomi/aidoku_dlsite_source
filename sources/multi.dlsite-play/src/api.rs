use crate::models::{DownloadToken, PurchaseWork, RawZipTree, SalesEntry, WorksResponse, ZipTree};
use crate::settings;
use aidoku::{
	alloc::{format, String, Vec},
	imports::net::Request,
	prelude::*,
	Result,
};
use core::str;

const PLAY_API: &str = "https://play.dlsite.com/api/v3";
const PLAY_DL_API: &str = "https://play.dl.dlsite.com/api/v3";
const REFERER: &str = "https://play.dlsite.com/";
const LOGIN_URL: &str = "https://login.dlsite.com/login";
const LOGIN_URL_WITH_USER: &str = "https://login.dlsite.com/login?user=self";
const PLAY_LOGIN_URL: &str = "https://play.dlsite.com/login/";
const PLAY_AUTHORIZE_URL: &str = "https://play.dlsite.com/api/authorize";
const LOGIN_SUCCESS_MARKER: &str = "ログイン中です";

fn play_get(url: &str) -> Result<Request> {
	Ok(Request::get(url)?.header("Referer", REFERER))
}

fn play_post(url: &str) -> Result<Request> {
	Ok(Request::post(url)?
		.header("Referer", REFERER)
		.header("Content-Type", "application/json"))
}

fn is_auth_expired_response(status_code: i32, data: &[u8]) -> bool {
	if status_code == 401 || status_code == 403 {
		return true;
	}
	if let Ok(body) = str::from_utf8(data) {
		if body.contains("login.dlsite.com/login")
			|| body.contains("\"message\":\"Unauthenticated\"")
			|| body.contains("\"error\":\"Unauthorized\"")
			|| (body.contains("<html") && body.contains("login"))
		{
			return true;
		}
	}
	false
}

fn percent_encode_component(value: &str) -> String {
	let mut encoded = String::new();
	for b in value.as_bytes() {
		let keep = b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.' | b'~');
		if keep {
			encoded.push(*b as char);
		} else {
			const HEX: &[u8; 16] = b"0123456789ABCDEF";
			encoded.push('%');
			encoded.push(HEX[(b >> 4) as usize] as char);
			encoded.push(HEX[(b & 0x0f) as usize] as char);
		}
	}
	encoded
}

fn extract_login_token(html: &str) -> Option<String> {
	let token_idx = html.find("name=\"_token\"")?;
	let after_name = &html[token_idx..];
	let value_key = "value=\"";
	let value_start = after_name.find(value_key)? + value_key.len();
	let rest = &after_name[value_start..];
	let value_end = rest.find('"')?;
	Some(rest[..value_end].into())
}

pub fn login(username: &str, password: &str) -> Result<()> {
	if username.is_empty() || password.is_empty() {
		bail!("Username and password are required.");
	}

	let login_page = Request::get(LOGIN_URL_WITH_USER)?.send()?;
	let login_page_data = login_page.get_data()?;
	let login_page_html =
		str::from_utf8(&login_page_data).map_err(|_| error!("Failed to decode login page"))?;
	let token = extract_login_token(login_page_html)
		.ok_or_else(|| error!("Failed to extract DLsite login form token"))?;

	let body = format!(
		"_token={}&login_id={}&password={}",
		percent_encode_component(&token),
		percent_encode_component(username),
		percent_encode_component(password)
	);
	let login_response = Request::post(LOGIN_URL)?
		.header("Referer", LOGIN_URL_WITH_USER)
		.header("Content-Type", "application/x-www-form-urlencoded")
		.body(body.as_bytes())
		.send()?;
	let login_response_data = login_response.get_data()?;
	let login_response_text = str::from_utf8(&login_response_data)
		.map_err(|_| error!("Failed to decode login response"))?;
	if !login_response_text.contains(LOGIN_SUCCESS_MARKER) {
		bail!("DLsite login failed. Please check your username and password.");
	}

	Request::get(PLAY_LOGIN_URL)?.send()?;
	play_get(PLAY_AUTHORIZE_URL)?.send()?;
	Ok(())
}

fn with_authenticated_data<F>(make_request: F) -> Result<Vec<u8>>
where
	F: Fn() -> Result<Request>,
{
	let response = make_request()?.send()?;
	let status_code = response.status_code();
	let data = response.get_data()?;
	if !is_auth_expired_response(status_code, &data) {
		return Ok(data);
	}

	let (username, password) = settings::get_credentials().ok_or_else(|| {
		settings::set_logged_in(false);
		error!("Session expired. Please log in again.")
	})?;
	if let Err(_e) = login(&username, &password) {
		settings::set_logged_in(false);
		settings::clear_cached_worknos();
		settings::clear_cached_page();
		bail!("Session expired and reauthentication failed. Please log in again.");
	}
	settings::set_logged_in(true);

	let retry_response = make_request()?.send()?;
	let retry_status_code = retry_response.status_code();
	let retry_data = retry_response.get_data()?;
	if is_auth_expired_response(retry_status_code, &retry_data) {
		settings::set_logged_in(false);
		settings::clear_cached_worknos();
		settings::clear_cached_page();
		bail!("Session expired and reauthentication failed. Please log in again.");
	}

	Ok(retry_data)
}

/// Fetch the list of purchased work IDs (sorted by sales date, newest first).
pub fn get_sales() -> Result<Vec<SalesEntry>> {
	let url = format!("{}/content/sales?last=0", PLAY_API);
	let data = with_authenticated_data(|| play_get(&url))?;
	let entries: Vec<SalesEntry> = serde_json::from_slice(&data).map_err(|e| {
		let body_preview = match str::from_utf8(&data) {
			Ok(s) => s,
			Err(_) => "<non-utf8 response body>",
		};
		error!(
			"Failed to parse sales response: {} ({} bytes). Body: {}",
			e,
			data.len(),
			body_preview
		)
	})?;
	Ok(entries)
}

/// Fetch full work metadata for a batch of work IDs.
/// The Play API accepts up to 100 work IDs per request.
pub fn get_works(worknos: &[String]) -> Result<Vec<PurchaseWork>> {
	let mut all_works: Vec<PurchaseWork> = Vec::new();

	for chunk in worknos.chunks(100) {
		let url = format!("{}/content/works", PLAY_API);
		let body = serde_json::to_vec(chunk).map_err(|_| error!("Failed to serialize work IDs"))?;
		let data = with_authenticated_data(|| Ok(play_post(&url)?.body(&body)))?;
		let resp: WorksResponse =
			serde_json::from_slice(&data).map_err(|_| error!("Failed to parse works response"))?;
		all_works.extend(resp.works);
	}

	Ok(all_works)
}

/// Get a download token for a specific work.
pub fn download_token(workno: &str) -> Result<DownloadToken> {
	let url = format!("{}/download/sign/cookie?workno={}", PLAY_DL_API, workno);
	let data = with_authenticated_data(|| play_get(&url))?;
	let token: DownloadToken =
		serde_json::from_slice(&data).map_err(|_| error!("Failed to parse download token"))?;
	Ok(token)
}

/// Fetch the ziptree for a download token.
pub fn fetch_ziptree(token: &DownloadToken) -> Result<ZipTree> {
	let url = format!("{}ziptree.json", token.url);
	let data = with_authenticated_data(|| play_get(&url))?;
	let raw: RawZipTree =
		serde_json::from_slice(&data).map_err(|_| error!("Failed to parse ziptree"))?;
	Ok(ZipTree::from_raw(raw))
}

/// Build the URL for downloading an optimized file.
pub fn optimized_url(token: &DownloadToken, optimized_name: &str) -> String {
	format!("{}optimized/{}", token.url, optimized_name)
}
