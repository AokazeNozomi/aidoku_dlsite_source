#![no_std]

use aidoku::{
	Chapter, FilterValue, HashMap, ImageRequestProvider, ImageResponse, Listing, ListingProvider,
	Manga, MangaPageResult, NotificationHandler, Page, PageContent, PageContext,
	PageImageProcessor, Result, Source, WebLoginHandler,
	alloc::{String, Vec, format, string::ToString, vec},
	imports::{canvas::ImageRef, net::Request},
	prelude::*,
	register_source,
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
		_filters: Vec<FilterValue>,
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

		if let Some(ref q) = query {
			// Fetch metadata for the full library so filtering covers everything
			let all_works = api::get_works(&worknos)?;
			let q_lower = q.to_lowercase();
			let filtered: Vec<Manga> = all_works
				.into_iter()
				.filter(|w| {
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
					name.contains(&q_lower)
						|| maker.contains(&q_lower)
						|| w.workno.contains(q.as_str())
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

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let works = api::get_works(&[manga.key.clone()])?;
			if let Some(work) = works.into_iter().next() {
				let updated: Manga = work.into();
				manga.copy_from(updated);
			}
		}

		if needs_chapters {
			let token = api::download_token(&manga.key)?;
			let ziptree = api::fetch_ziptree(&token)?;
			let pages = helpers::extract_pages(&ziptree);
			let page_count = pages.len();

			manga.chapters = Some(vec![Chapter {
				key: manga.key.clone(),
				title: Some(format!("{} pages", page_count)),
				chapter_number: Some(1.0),
				..Default::default()
			}]);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, _chapter: Chapter) -> Result<Vec<Page>> {
		let token = api::download_token(&manga.key)?;
		let ziptree = api::fetch_ziptree(&token)?;
		let pages = helpers::extract_pages(&ziptree);

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
		match listing.id.as_str() {
			"purchases" => self.get_search_manga_list(None, page, Vec::new()),
			_ => Err(error!("Unknown listing: {}", listing.id)),
		}
	}
}

impl WebLoginHandler for DlsitePlay {
	fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
		if key != "login" {
			bail!("Invalid login key: `{key}`");
		}

		let has_session = cookies.keys().any(|k: &String| {
			k.contains("DLsite_SID")
				|| k.contains("login_secure")
				|| k.contains("__DLsite")
				|| k == "PHPSESSID"
		});

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
		if notification == "login" && !settings::is_logged_in() {
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

/// Fetch (or use cached) full purchase work ID list, refreshing on page 1.
fn get_or_fetch_worknos(page: i32) -> Result<Vec<String>> {
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

register_source!(
	DlsitePlay,
	ListingProvider,
	WebLoginHandler,
	NotificationHandler,
	ImageRequestProvider,
	PageImageProcessor
);
