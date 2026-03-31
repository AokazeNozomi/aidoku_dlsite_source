#![no_std]

use aidoku::{
	alloc::{format, string::ToString, vec, String, Vec},
	imports::{canvas::ImageRef, net::Request, std::{current_date, print}},
	prelude::*,
	register_source, Chapter, FilterValue, HashMap, ImageRequestProvider, ImageResponse, Listing,
	ListingProvider, Manga, MangaPageResult, NotificationHandler, Page, PageContent, PageContext,
	PageImageProcessor, Result, Source, WebLoginHandler,
};

mod api;
mod helpers;
mod models;
mod settings;

const PAGE_SIZE: usize = 20;
/// Skip duplicate `/content/sales` calls when Aidoku requests page 1 several times in a row.
const SALES_CACHE_MAX_AGE_SEC: i64 = 120;

struct DlsitePlay;

impl Source for DlsitePlay {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let work_types = extract_work_type_filter(&filters);
		let translation_filter = extract_translation_filter(&filters);
		get_manga_list_inner(query, page, work_types, translation_filter)
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let mut release_date: Option<i64> = None;

		if needs_details || needs_chapters {
			let works = api::get_works(&[manga.key.clone()])?;
			if let Some(work) = works.into_iter().next() {
				release_date = work.release_date_timestamp();
				if needs_details {
					let updated: Manga = work.into();
					manga.copy_from(updated);

					// Enrich with language editions from public API (cached)
					if let Some(lang_str) = get_or_fetch_languages(&manga.key) {
						let mut tags = manga.tags.take().unwrap_or_default();
						for part in lang_str.split(',') {
							if let Some((_code, label)) = part.split_once(':') {
								let tag = format!("Lang: {}", label);
								if !tags.contains(&tag) {
									tags.push(tag);
								}
							}
						}
						manga.tags = Some(tags);

						let labels: Vec<&str> = lang_str
							.split(',')
							.filter_map(|s| s.split_once(':').map(|(_, l)| l))
							.collect();
						if !labels.is_empty() {
							let mut desc = manga.description.take().unwrap_or_default();
							desc.push('\n');
							desc.push_str(&format!("Languages: {}", labels.join(", ")));
							manga.description = Some(desc);
						}
					}
				}
			}
		}

		if needs_chapters {
			let token = api::download_token(&manga.key)?;
			let ziptree = api::fetch_ziptree(&token)?;
			let chapter_groups = helpers::extract_chapter_groups(&ziptree);

			let chapters: Vec<Chapter> = chapter_groups
				.into_iter()
				.enumerate()
				.map(|(idx, group)| Chapter {
					key: group.key,
					title: Some(format!("{} ({} pages)", group.title, group.pages.len())),
					chapter_number: Some((idx + 1) as f32),
					date_uploaded: release_date,
					..Default::default()
				})
				.collect();

			manga.chapters = Some(chapters);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let token = api::download_token(&manga.key)?;
		let ziptree = api::fetch_ziptree(&token)?;
		let chapter_groups = helpers::extract_chapter_groups(&ziptree);
		let pages = chapter_groups
			.into_iter()
			.find(|group| group.key == chapter.key)
			.map(|group| group.pages)
			.ok_or_else(|| error!("Unable to find chapter pages for key '{}'", chapter.key))?;

		let result: Vec<Page> = pages
			.into_iter()
			.map(|(_path, pf)| {
				let opt_name = pf.optimized_name().unwrap_or_default().to_string();
				let url = api::optimized_url(&token, &opt_name);

				let mut context = PageContext::new();
				context.insert("optimized_name".into(), opt_name);

				if pf.is_crypt() {
					context.insert("crypt".into(), "true".into());
					if let Some((w, h)) = pf.crypt_dimensions() {
						context.insert("width".into(), w.to_string());
						context.insert("height".into(), h.to_string());
					}
				}

				Page {
					content: PageContent::url_context(url, context),
					..Default::default()
				}
			})
			.collect();

		Ok(result)
	}
}

impl ListingProvider for DlsitePlay {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let work_types = match listing.id.as_str() {
			"purchases" => Vec::new(),
			wt => vec![wt.to_string()],
		};
		get_manga_list_inner(None, page, work_types, None)
	}
}

impl WebLoginHandler for DlsitePlay {
	fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
		if key != "login" {
			print(format!(
				"[dlsite-play] web login rejected invalid key `{key}`"
			));
			bail!("Invalid login key: `{key}`");
		}

		let play_session = cookies.get("play_session");

		// Aidoku's WebView fires cookiesDidChange continuously. The initial
		// redirect sets an encrypted play_session (Laravel EncryptCookies
		// middleware — value starts with "eyJ", base64 for `{"iv":…`).
		// Returning true here dismisses the WebView, which kills the SPA
		// before it can call /api/authorize to bind the session.
		//
		// Wait for the SPA's /api/authorize to replace the cookie with a
		// plain session ID (~40 chars). Aidoku calls handle_web_login again
		// when the cookie changes.
		if let Some(ps) = play_session {
			if ps.starts_with("eyJ") {
				print(format!(
					"[dlsite-play] web login: play_session still encrypted ({} chars), waiting for SPA to authorize",
					ps.len()
				));
				return Ok(false);
			}
		}

		let mut keys: Vec<&str> = cookies.keys().map(|s| s.as_str()).collect();
		keys.sort();
		let mut cookie_pairs: Vec<String> = Vec::new();
		for name in &keys {
			if let Some(value) = cookies.get(*name) {
				cookie_pairs.push(format!("{}={}", name, value));
			}
		}

		let has_session = play_session.is_some();
		print(format!(
			"[dlsite-play] web login summary count={} has_play_session={} session_len={}",
			cookies.len(),
			has_session,
			play_session.map(|s| s.len()).unwrap_or(0),
		));

		settings::set_logged_in(has_session);

		if has_session {
			let cookie_header = cookie_pairs.join("; ");
			settings::set_web_cookies(&cookie_header);
			print(format!(
				"[dlsite-play] web login stored Cookie header ({} chars)",
				cookie_header.len()
			));
			settings::clear_cached_worknos();
			settings::clear_cached_page();
		} else {
			settings::clear_web_cookies();
		}

		Ok(has_session)
	}
}

impl NotificationHandler for DlsitePlay {
	fn handle_notification(&self, notification: String) {
		if notification == "login" && !settings::is_logged_in() {
			settings::clear_cached_worknos();
			settings::clear_cached_page();
		}
	}
}

impl ImageRequestProvider for DlsitePlay {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		api::play_image_get(&url)
	}
}

impl PageImageProcessor for DlsitePlay {
	fn process_page_image(
		&self,
		response: ImageResponse,
		context: Option<PageContext>,
	) -> Result<ImageRef> {
		let Some(ctx) = context else {
			return Ok(response.image);
		};

		let is_crypt = ctx
			.get("crypt")
			.map(|s| s.as_str() == "true")
			.unwrap_or(false);
		if !is_crypt {
			return Ok(response.image);
		}

		let opt_name = ctx.get("optimized_name").cloned().unwrap_or_default();
		let width = ctx
			.get("width")
			.and_then(|s| s.parse::<i32>().ok())
			.unwrap_or_else(|| response.image.width() as i32);
		let height = ctx
			.get("height")
			.and_then(|s| s.parse::<i32>().ok())
			.unwrap_or_else(|| response.image.height() as i32);

		if width == 0 || height == 0 {
			return Ok(response.image);
		}

		match helpers::descramble_image(&response.image, &opt_name, width, height) {
			Ok(img) => Ok(img),
			Err(_) => Ok(response.image),
		}
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Core listing/search implementation shared by search and listing providers.
fn get_manga_list_inner(
	query: Option<String>,
	page: i32,
	work_types: Vec<String>,
	translation_filter: Option<String>,
) -> Result<MangaPageResult> {
	let worknos = get_or_fetch_worknos(page)?;

	if worknos.is_empty() {
		return Ok(MangaPageResult {
			entries: Vec::new(),
			has_next_page: false,
		});
	}

	let page_idx = (page - 1).max(0) as usize;
	let start = page_idx * PAGE_SIZE;

	let has_query = query.is_some();
	let has_filter = !work_types.is_empty() || translation_filter.is_some();

	if has_query || has_filter {
		let all_works = api::get_works(&worknos)?;
		let q_lower = query.as_ref().map(|q| q.to_lowercase());

		let filtered: Vec<Manga> = all_works
			.into_iter()
			.filter(|w| {
				if let Some(ref q_lower) = q_lower {
					let name = w
						.name
						.as_ref()
						.map(|n| n.best())
						.unwrap_or_default()
						.to_lowercase();
					let maker = w
						.maker
						.as_ref()
						.and_then(|m| m.name.as_ref())
						.map(|n| n.best())
						.unwrap_or_default()
						.to_lowercase();
					let q_raw = query.as_deref().unwrap_or_default();
					if !(name.contains(q_lower.as_str())
						|| maker.contains(q_lower.as_str())
						|| w.workno.contains(q_raw))
					{
						return false;
					}
				}
				if !work_types.is_empty() {
					let wt = w.work_type.as_deref().unwrap_or("");
					if !work_types.iter().any(|t| t == wt) {
						return false;
					}
				}
				if let Some(ref tf) = translation_filter {
					let is_translated = w.has_translator();
					match tf.as_str() {
						"translated" if !is_translated => return false,
						"original" if is_translated => return false,
						_ => {}
					}
				}
				true
			})
			.map(|w| w.into())
			.collect();

		let total = filtered.len();
		if start >= total {
			return Ok(MangaPageResult {
				entries: Vec::new(),
				has_next_page: false,
			});
		}

		let end = (start + PAGE_SIZE).min(total);
		let entries: Vec<Manga> = filtered.into_iter().skip(start).take(end - start).collect();

		Ok(MangaPageResult {
			entries,
			has_next_page: end < total,
		})
	} else {
		if start >= worknos.len() {
			return Ok(MangaPageResult {
				entries: Vec::new(),
				has_next_page: false,
			});
		}

		let end = (start + PAGE_SIZE).min(worknos.len());
		let page_worknos: Vec<String> = worknos[start..end].to_vec();
		let works = api::get_works(&page_worknos)?;
		let entries: Vec<Manga> = works.into_iter().map(|w| w.into()).collect();

		Ok(MangaPageResult {
			entries,
			has_next_page: end < worknos.len(),
		})
	}
}

/// Fetch (or use cached) full purchase work ID list, refreshing on page 1.
fn get_or_fetch_worknos(page: i32) -> Result<Vec<String>> {
	print(format!(
		"[dlsite-play] get_or_fetch_worknos page={} logged_in={}",
		page,
		settings::is_logged_in()
	));
	if !settings::is_logged_in() {
		print(format!(
			"[dlsite-play] get_or_fetch_worknos skip: not logged in (Account → Login)"
		));
		return Ok(Vec::new());
	}
	if page == 1 {
		let now = current_date();
		let cached = settings::get_cached_worknos();
		let fetched_at = settings::get_sales_fetched_at();
		let cache_fresh = fetched_at
			.map(|t| now.saturating_sub(t) < SALES_CACHE_MAX_AGE_SEC)
			.unwrap_or(false);
		if !cached.is_empty() && cache_fresh {
			print(format!(
				"[dlsite-play] get_or_fetch_worknos page=1 using sales cache age={}s count={}",
				fetched_at.map(|t| now.saturating_sub(t)).unwrap_or(0),
				cached.len()
			));
			return Ok(cached);
		}

		let sales = api::get_sales()?;
		let worknos: Vec<String> = sales.into_iter().map(|s| s.workno).collect();
		print(format!(
			"[dlsite-play] refreshed sales list count={}",
			worknos.len()
		));
		settings::set_cached_worknos(&worknos);
		settings::set_sales_fetched_at(now);
		Ok(worknos)
	} else {
		let cached = settings::get_cached_worknos();
		if cached.is_empty() {
			print("[dlsite-play] cache empty, fetching sales");
			let sales = api::get_sales()?;
			let worknos: Vec<String> = sales.into_iter().map(|s| s.workno).collect();
			print(format!(
				"[dlsite-play] sales list count={} (was empty cache)",
				worknos.len()
			));
			settings::set_cached_worknos(&worknos);
			settings::set_sales_fetched_at(current_date());
			Ok(worknos)
		} else {
			print(format!(
				"[dlsite-play] using cached worknos count={}",
				cached.len()
			));
			Ok(cached)
		}
	}
}

/// Fetch language editions from cache or public API. Empty results are not
/// cached so region-locked lookups can be retried later (e.g. via VPN).
fn get_or_fetch_languages(workno: &str) -> Option<String> {
	if let Some(cached) = settings::get_cached_languages(workno) {
		return Some(cached);
	}
	let editions = api::get_language_editions(workno).ok()?;
	if editions.is_empty() {
		return None;
	}
	let pairs: Vec<String> = editions
		.iter()
		.map(|e| format!("{}:{}", e.lang, e.label))
		.collect();
	let value = pairs.join(",");
	settings::set_cached_languages(workno, &value);
	Some(value)
}

fn extract_translation_filter(filters: &[FilterValue]) -> Option<String> {
	for f in filters {
		if let FilterValue::Select { id, value } = f {
			if id == "translation" && value != "all" {
				return Some(value.clone());
			}
		}
	}
	None
}

fn extract_work_type_filter(filters: &[FilterValue]) -> Vec<String> {
	for f in filters {
		if let FilterValue::MultiSelect { id, included, .. } = f {
			if id == "work_type" && !included.is_empty() {
				return included.clone();
			}
		}
	}
	Vec::new()
}

register_source!(
	DlsitePlay,
	ListingProvider,
	WebLoginHandler,
	NotificationHandler,
	ImageRequestProvider,
	PageImageProcessor
);
