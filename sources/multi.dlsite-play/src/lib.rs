#![no_std]

use aidoku::{
	alloc::{format, string::ToString, vec, String, Vec},
	imports::{canvas::ImageRef, net::Request, std::print},
	prelude::*,
	register_source, BasicLoginHandler, Chapter, FilterValue, HashMap, ImageRequestProvider,
	ImageResponse, Listing, ListingProvider, Manga, MangaPageResult, NotificationHandler, Page,
	PageContent, PageContext, PageImageProcessor, Result, Source, WebLoginHandler,
};

mod api;
mod helpers;
mod models;
mod settings;

const PAGE_SIZE: usize = 20;

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
		get_manga_list_inner(query, page, work_types)
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let mut language: Option<String> = None;
		let mut release_date: Option<i64> = None;

		if needs_details || needs_chapters {
			let works = api::get_works(&[manga.key.clone()])?;
			if let Some(work) = works.into_iter().next() {
				language = work.infer_language().map(String::from);
				release_date = work.release_date_timestamp();
				if needs_details {
					let updated: Manga = work.into();
					manga.copy_from(updated);
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
					language: language.clone(),
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
		get_manga_list_inner(None, page, work_types)
	}
}

impl BasicLoginHandler for DlsitePlay {
	fn handle_basic_login(&self, key: String, username: String, password: String) -> Result<bool> {
		print(format!("[dlsite-play] handle_basic_login called with key={}", key));
		if key != "login" {
			bail!("Invalid login key: `{key}`");
		}

		// Persist credentials first; several sources rely on these defaults keys.
		settings::set_credentials(&username, &password);
		print(format!(
			"[dlsite-play] stored credentials (username_len={}, password_len={})",
			username.len(),
			password.len()
		));
		api::login(&username, &password)?;
		print("[dlsite-play] api::login succeeded");
		settings::set_logged_in(true);
		settings::clear_cached_worknos();
		settings::clear_cached_page();
		print("[dlsite-play] login state + caches updated");
		Ok(true)
	}
}

impl WebLoginHandler for DlsitePlay {
	fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
		print(format!("[dlsite-play] handle_web_login called with key={}", key));
		if key != "login_web" {
			bail!("Invalid web login key: `{key}`");
		}

		let has_session =
			cookies.contains_key("play_session") || cookies.contains_key("PHPSESSID");
		print(format!(
			"[dlsite-play] web login cookies received={}, has_session={}",
			cookies.len(),
			has_session
		));

		settings::set_logged_in(has_session);
		if has_session {
			settings::clear_cached_worknos();
			settings::clear_cached_page();
		}

		Ok(has_session)
	}
}

impl NotificationHandler for DlsitePlay {
	fn handle_notification(&self, notification: String) {
		if (notification == "login" || notification == "login_web") && !settings::is_logged_in() {
			settings::clear_cached_worknos();
			settings::clear_cached_page();
		}
	}
}

impl ImageRequestProvider for DlsitePlay {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", "https://play.dlsite.com/"))
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
	let has_filter = !work_types.is_empty();

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
				if has_filter {
					let wt = w.work_type.as_deref().unwrap_or("");
					if !work_types.iter().any(|t| t == wt) {
						return false;
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
		"[dlsite-play] get_or_fetch_worknos page={} is_logged_in={}",
		page,
		settings::is_logged_in()
	));
	if !settings::is_logged_in() {
		if let Some((username, password)) = settings::get_credentials() {
			print("[dlsite-play] attempting lazy login from stored credentials");
			api::login(&username, &password)?;
			settings::set_logged_in(true);
			print("[dlsite-play] lazy login succeeded");
		} else {
			print("[dlsite-play] no stored credentials available");
			bail!("Not logged in. Please log in to view your purchases.");
		}
	}

	if page == 1 {
		let sales = api::get_sales()?;
		let worknos: Vec<String> = sales.into_iter().map(|s| s.workno).collect();
		settings::set_cached_worknos(&worknos);
		Ok(worknos)
	} else {
		let cached = settings::get_cached_worknos();
		if cached.is_empty() {
			let sales = api::get_sales()?;
			let worknos: Vec<String> = sales.into_iter().map(|s| s.workno).collect();
			settings::set_cached_worknos(&worknos);
			Ok(worknos)
		} else {
			Ok(cached)
		}
	}
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
	BasicLoginHandler,
	WebLoginHandler,
	NotificationHandler,
	ImageRequestProvider,
	PageImageProcessor
);
