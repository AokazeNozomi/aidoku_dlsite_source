#![no_std]

use aidoku::{
	alloc::{collections::{BTreeMap, BTreeSet}, format, string::ToString, String, Vec},
	imports::{canvas::ImageRef, net::{Request, set_rate_limit, TimeUnit}, std::{current_date, print}},
	prelude::*,
	register_source, Chapter, FilterValue, HashMap, Home, HomeComponent, HomeComponentValue,
	HomeLayout, ImageRequestProvider, ImageResponse, Link, Listing, ListingKind, ListingProvider,
	Manga, MangaPageResult, NotificationHandler, Page, PageContent, PageContext, PageImageProcessor,
	Result, Source, WebLoginHandler,
};

mod models;
mod play;
mod public;
mod settings;

/// DLsite sub-site slug used for public API calls.
const DLSITE_SITE_SLUG: &str = "maniax";

const PAGE_SIZE: usize = 20;
/// Skip duplicate `/content/sales` calls when Aidoku requests page 1 several times in a row.
const SALES_CACHE_MAX_AGE_SEC: i64 = 120;

use settings::SortOption;

struct SortKey {
	/// Position in the original worknos array (purchase order).
	original_position: usize,
	/// `accessed_at` from view_histories (empty if not in history).
	recently_opened: String,
	/// `regist_date` from the work.
	release_date: String,
	/// Maker/circle name (lowercased for case-insensitive sort).
	writer_name: String,
	/// Title (lowercased for case-insensitive sort).
	title: String,
}

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
		let (work_types, work_type_exclude) = extract_work_type_filter(&filters);
		let translation_filter = extract_translation_filter(&filters);
		let (genre_filter, genre_exclude) = extract_genre_filter(&filters);
		let content_rating_filter = extract_content_rating_filter(&filters);
		let (sort_option, sort_ascending) = extract_sort_filter(&filters);
		get_manga_list_inner(query, page, work_types, work_type_exclude, translation_filter, genre_filter, genre_exclude, content_rating_filter, sort_option, sort_ascending)
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let is_series = manga.key.starts_with("series:");

		if is_series {
			self.update_series_manga(&mut manga, needs_details, needs_chapters)?;
		} else {
			self.update_single_manga(&mut manga, needs_details, needs_chapters)?;
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		// For series chapters, the key is "{workno}:{internal_key}".
		// For non-series chapters, the key is just the internal key and
		// the workno comes from manga.key.
		let (workno, chapter_key) = if manga.key.starts_with("series:") {
			split_series_chapter_key(&chapter.key)
		} else {
			(manga.key.as_str(), chapter.key.as_str())
		};

		let token = play::download_token(workno)?;
		let ziptree = play::fetch_ziptree(&token)?;
		let chapter_groups = play::extract_chapter_groups(&ziptree);
		let pages = chapter_groups
			.into_iter()
			.find(|group| group.key == chapter_key)
			.map(|group| group.pages)
			.ok_or_else(|| error!("Unable to find chapter pages for key '{}'", chapter.key))?;

		let mut result: Vec<Page> = Vec::new();

		// Prepend the work's cover image as the first page of the first
		// chapter (chapter_number == 1). For series entries each volume's
		// first chapter gets its own cover.
		let is_first_chapter = chapter
			.chapter_number
			.map(|n| (n - 1.0).abs() < 0.01)
			.unwrap_or(false);
		if is_first_chapter {
			if let Some(cover_url) = work_cover_url(workno) {
				result.push(Page {
					content: PageContent::url(cover_url),
					..Default::default()
				});
			}
		}

		result.extend(pages.into_iter().map(|(_path, pf)| {
			let opt_name = pf.optimized_name().unwrap_or_default().to_string();
			let url = play::optimized_url(&token, &opt_name);

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
		}));

		// Update "recently opened" on DLsite (fire-and-forget).
		// Throttle: skip if we already touched this workno recently
		// (avoids spamming during bulk downloads).
		touch_view_history(workno);

		Ok(result)
	}
}

impl DlsitePlay {
	/// Update a single (non-series) manga entry.
	fn update_single_manga(
		&self,
		manga: &mut Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<()> {
		let mut release_date: Option<i64> = None;
		let mut is_purchased = false;

		if needs_details || needs_chapters {
			// Try the authenticated Play API first.
			let resp = play::get_works(&[manga.key.clone()]).ok();
			let play_work = resp.and_then(|mut r| {
				let series = core::mem::take(&mut r.series);
				r.works.into_iter().next().map(|w| (w, series))
			});

			if let Some((work, series)) = play_work {
				// get_works returns metadata for any work when logged in,
				// so check the sales cache to confirm actual purchase status.
				let cached_worknos = settings::get_cached_worknos();
				is_purchased = cached_worknos.iter().any(|w| w == &work.workno);
				release_date = work.release_date_timestamp();
				if needs_details {
					let genre_names = resolve_genre_names(&[&work]);
					let series_names = build_series_lookup(&series);
					let updated = work.into_manga(&genre_names, &series_names);
					manga.copy_from(updated);

					if let Some(label) = get_work_language(&manga.key) {
						let mut tags = manga.tags.take().unwrap_or_default();
						let tag = format!("Lang: {}", label);
						if !tags.contains(&tag) {
							tags.push(tag);
						}
						manga.tags = Some(tags);

						let mut desc = manga.description.take().unwrap_or_default();
						desc.push('\n');
						desc.push_str(&format!("Language: {}", label));
						manga.description = Some(desc);
					}
				}
			} else if needs_details {
				// Fallback to public product API for non-purchased works.
				let locale = settings::get_preferred_language().locale_code();
				if let Ok(Some(public_work)) =
					public::get_public_work_details(DLSITE_SITE_SLUG, &manga.key, Some(locale))
				{
					let updated = public_work.into_manga(DLSITE_SITE_SLUG);
					manga.copy_from(updated);
				}
			}
		}

		// Chapters are only available for purchased works.
		if needs_chapters && is_purchased {
			let token = play::download_token(&manga.key)?;
			let ziptree = play::fetch_ziptree(&token)?;
			let chapter_groups = play::extract_chapter_groups(&ziptree);

			let chapters: Vec<Chapter> = chapter_groups
				.into_iter()
				.enumerate()
				.map(|(idx, group)| {
					let url = group
						.pages
						.first()
						.map(|(path, _)| play_viewer_url(&manga.key, path));
					Chapter {
						key: group.key,
						title: Some(format!("{} ({} pages)", group.title, group.pages.len())),
						chapter_number: Some((idx + 1) as f32),
						date_uploaded: release_date,
						url,
						..Default::default()
					}
				})
				.collect();

			manga.chapters = Some(chapters);
		}

		Ok(())
	}

	/// Update a series manga entry: fetch all member works, build chapters.
	fn update_series_manga(
		&self,
		manga: &mut Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<()> {
		let title_id = manga
			.key
			.strip_prefix("series:")
			.unwrap_or(&manga.key)
			.to_string();

		let member_worknos = settings::get_cached_series_map(&title_id);
		if member_worknos.is_empty() {
			print(format!(
				"[dlsite-play] series {} has no cached members",
				title_id
			));
			return Ok(());
		}

		let resp = play::get_works(&member_worknos)?;
		let mut works = resp.works;
		sort_works_by_volume(&mut works);

		if needs_details {
			let work_refs: Vec<&models::PurchaseWork> = works.iter().collect();
			let genre_names = resolve_genre_names(&work_refs);
			let series_names = build_series_lookup(&resp.series);
			let sname = series_names
				.get(&title_id)
				.cloned()
				.or_else(|| models::derive_series_name(&works))
				.unwrap_or_else(|| title_id.clone());
			let updated = models::series_manga(&title_id, &sname, &works, &genre_names);
			manga.copy_from(updated);

			// Collect languages from all member works (purchased editions only).
			let mut seen = BTreeSet::new();
			let mut tags = manga.tags.take().unwrap_or_default();
			let mut lang_labels: Vec<String> = Vec::new();
			for w in &works {
				if let Some(label) = get_work_language(&w.workno) {
					if seen.insert(label.clone()) {
						let tag = format!("Lang: {}", label);
						if !tags.contains(&tag) {
							tags.push(tag);
						}
						lang_labels.push(label);
					}
				}
			}
			manga.tags = Some(tags);
			if !lang_labels.is_empty() {
				let mut desc = manga.description.take().unwrap_or_default();
				desc.push('\n');
				desc.push_str(&format!("Languages: {}", lang_labels.join(", ")));
				manga.description = Some(desc);
			}
		}

		if needs_chapters {
			let mut chapters: Vec<Chapter> = Vec::new();

			for (vol_idx, work) in works.iter().enumerate() {
				let vol_num = work
					.series
					.as_ref()
					.and_then(|s| s.volume_number)
					.map(|v| v as f32)
					.unwrap_or((vol_idx + 1) as f32);
				let release_date = work.release_date_timestamp();
				let work_title = work
					.name
					.as_ref()
					.map(|n| n.best())
					.unwrap_or_else(|| work.workno.clone());

				let language = get_work_language(&work.workno);

				let token = play::download_token(&work.workno)?;
				let ziptree = play::fetch_ziptree(&token)?;
				let chapter_groups = play::extract_chapter_groups(&ziptree);
				let num_groups = chapter_groups.len();

				for (ch_idx, group) in chapter_groups.into_iter().enumerate() {
					let title = if num_groups == 1 {
						// For works with a single chapter group, just use the
						// work title as the chapter title — the `volume_number`
						// already carries the volume information.
						format!("{} ({} pages)", work_title, group.pages.len())
					} else {
						format!(
							"{} — {} ({} pages)",
							work_title,
							group.title,
							group.pages.len()
						)
					};

					let url = group
						.pages
						.first()
						.map(|(path, _)| play_viewer_url(&work.workno, path));

					chapters.push(Chapter {
						key: format!("{}:{}", work.workno, group.key),
						title: Some(title),
						volume_number: Some(vol_num),
						chapter_number: Some((ch_idx + 1) as f32),
						date_uploaded: release_date,
						url,
						language: language.clone(),
						..Default::default()
					});
				}
			}

			manga.chapters = Some(chapters);
		}

		Ok(())
	}
}

impl ListingProvider for DlsitePlay {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		match listing.id.as_str() {
			"library" => {
				if !settings::is_logged_in() {
					bail!("Not logged in. Log in from the source settings to view your library.");
				}
				let work_types = settings::get_work_type_setting();
				let content_rating_filter = settings_content_rating_to_filter();
				let sort_option = settings::get_default_sort();
				let sort_ascending = settings::get_default_sort_ascending();
				get_manga_list_inner(None, page, work_types, Vec::new(), None, Vec::new(), Vec::new(), content_rating_filter, sort_option, sort_ascending)
			}
			_ => Ok(MangaPageResult {
				entries: Vec::new(),
				has_next_page: false,
			}),
		}
	}
}

impl Home for DlsitePlay {
	fn get_home(&self) -> Result<HomeLayout> {
		if !settings::is_logged_in() {
			return Ok(HomeLayout {
				components: Vec::from([HomeComponent {
					title: Some(String::from("Not Logged In")),
					subtitle: Some(String::from(
						"Log in from the source settings to view your library.",
					)),
					value: HomeComponentValue::empty_scroller(),
				}]),
			});
		}

		let work_types = settings::get_work_type_setting();
		let content_rating_filter = settings_content_rating_to_filter();
		let worknos = get_or_fetch_worknos(1)?;

		if worknos.is_empty() {
			return Ok(HomeLayout { components: Vec::new() });
		}

		let resp = play::get_works(&worknos)?;

		// --- Recently Released carousel ---
		// Individual works sorted by regist_date descending, deduplicated by
		// series title_id, top 20.
		let work_refs: Vec<&models::PurchaseWork> = resp.works.iter().collect();
		let genre_names = resolve_genre_names(&work_refs);
		let series_names = build_series_lookup(&resp.series);

		let mut recent: Vec<&models::PurchaseWork> = resp.works.iter()
			.filter(|w| {
				if !work_types.is_empty() {
					if !w.work_type.as_deref()
						.map(|wt| work_types.iter().any(|t| t == wt))
						.unwrap_or(false)
					{
						return false;
					}
				}
				if !w.matches_content_ratings(&content_rating_filter) {
					return false;
				}
				true
			})
			.collect();
		recent.sort_by(|a, b| {
			let ad = a.regist_date.as_deref().unwrap_or("");
			let bd = b.regist_date.as_deref().unwrap_or("");
			bd.cmp(ad)
		});

		let mut seen_series: Vec<String> = Vec::new();
		let mut carousel_entries: Vec<Link> = Vec::new();
		for w in &recent {
			if carousel_entries.len() >= 20 {
				break;
			}
			if let Some(ref ws) = w.series {
				if seen_series.contains(&ws.title_id) {
					continue;
				}
				seen_series.push(ws.title_id.clone());
			}
			carousel_entries.push((*w).clone().into_manga(&genre_names, &series_names).into());
		}

		// --- Recently Read carousel ---
		// Works sorted by view history accessed_at descending, top 20,
		// respecting work type filter.
		let mut read_entries: Vec<Link> = Vec::new();
		if let Ok(view_hist) = play::get_view_histories() {
			let mut hist_sorted = view_hist;
			hist_sorted.sort_by(|a, b| {
				let aa = a.accessed_at.as_deref().unwrap_or("");
				let ba = b.accessed_at.as_deref().unwrap_or("");
				ba.cmp(aa)
			});
			for entry in &hist_sorted {
				if read_entries.len() >= 20 {
					break;
				}
				if let Some(w) = resp.works.iter().find(|w| w.workno == entry.workno) {
					if !work_types.is_empty() {
						if !w.work_type.as_deref()
							.map(|wt| work_types.iter().any(|t| t == wt))
							.unwrap_or(false)
						{
							continue;
						}
					}
					if !w.matches_content_ratings(&content_rating_filter) {
						continue;
					}
					read_entries.push(w.clone().into_manga(&genre_names, &series_names).into());
				}
			}
		}

		// --- Library preview ---
		let sort_option = settings::get_default_sort();
		let sort_ascending = settings::get_default_sort_ascending();
		let library_entries = build_sorted_entries(
			&worknos, resp, None, work_types, Vec::new(), None, Vec::new(),
			Vec::new(), content_rating_filter, sort_option, sort_ascending,
		);
		let library_links: Vec<Link> = library_entries
			.into_iter()
			.take(PAGE_SIZE)
			.map(|(_, manga)| manga.into())
			.collect();

		let mut components = Vec::new();

		if !carousel_entries.is_empty() {
			components.push(HomeComponent {
				title: Some(String::from("Recently Released")),
				subtitle: None,
				value: HomeComponentValue::Scroller {
					entries: carousel_entries,
					listing: None,
				},
			});
		}

		if !read_entries.is_empty() {
			components.push(HomeComponent {
				title: Some(String::from("Recently Read")),
				subtitle: None,
				value: HomeComponentValue::Scroller {
					entries: read_entries,
					listing: None,
				},
			});
		}

		if !library_links.is_empty() {
			components.push(HomeComponent {
				title: Some(String::from("Library")),
				subtitle: None,
				value: HomeComponentValue::MangaList {
					ranking: false,
					page_size: None,
					entries: library_links,
					listing: Some(Listing {
						id: String::from("library"),
						name: String::from("Library"),
						kind: ListingKind::default(),
					}),
				},
			});
		}

		Ok(HomeLayout { components })
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
		match notification.as_str() {
			"login" if !settings::is_logged_in() => {
				settings::clear_cached_worknos();
				settings::clear_cached_page();
			}
			"fetch_languages" => {
				fetch_all_languages();
			}
			_ => {}
		}
	}
}

impl ImageRequestProvider for DlsitePlay {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		play::play_image_get(&url)
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

		match play::descramble_image(&response.image, &opt_name, width, height) {
			Ok(img) => Ok(img),
			Err(_) => Ok(response.image),
		}
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Post view history, but only if enabled and a different work than last time.
fn touch_view_history(workno: &str) {
	if !settings::update_view_history_enabled() {
		return;
	}
	if settings::get_last_viewed_workno().as_deref() == Some(workno) {
		return;
	}
	settings::set_last_viewed_workno(workno);
	let _ = play::post_view_history(workno);
}

/// Split a series chapter key `"{workno}:{internal_key}"` into its components.
/// The workno is the leading segment matching `[A-Z]+[0-9]+` (e.g. `RJ274802`),
/// and the internal key is everything after the first `:` (e.g. `img:root`).
fn split_series_chapter_key(key: &str) -> (&str, &str) {
	// Find the first ':' that follows the workno prefix.
	// Worknos are like RJ274802, BJ295623, VJ01006082 — letters then digits.
	if let Some(pos) = key.find(':') {
		(&key[..pos], &key[pos + 1..])
	} else {
		(key, "")
	}
}

/// Get the cover image URL for a work from the Play API.
fn work_cover_url(workno: &str) -> Option<String> {
	let resp = play::get_works(&[workno.into()]).ok()?;
	resp.works.into_iter().next()?.cover_url()
}

/// Percent-encode a path for use in a DLsite Play URL.
/// Encodes all bytes except unreserved chars (A-Z, a-z, 0-9, `-`, `_`, `.`, `~`).
fn percent_encode_path(s: &str) -> String {
	let mut out = String::new();
	for &b in s.as_bytes() {
		match b {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
				out.push(b as char);
			}
			_ => {
				out.push('%');
				const HEX: &[u8; 16] = b"0123456789ABCDEF";
				out.push(HEX[(b >> 4) as usize] as char);
				out.push(HEX[(b & 0x0f) as usize] as char);
			}
		}
	}
	out
}

/// Build a DLsite Play viewer URL for a specific file within a work.
fn play_viewer_url(workno: &str, file_path: &str) -> String {
	format!(
		"https://play.dlsite.com/work/{}/view/{}",
		workno,
		percent_encode_path(file_path)
	)
}

/// Resolve genre IDs to localized names, using the persistent cache.
/// Fetches only IDs not already in the cache.
fn resolve_genre_names(works: &[&models::PurchaseWork]) -> BTreeMap<u32, String> {
	let mut lookup = BTreeMap::new();

	// Load existing cache
	if let Some(cached) = settings::get_cached_genres() {
		for pair in cached.split('\n') {
			if let Some((id_str, name)) = pair.split_once(':') {
				if let Ok(id) = id_str.parse::<u32>() {
					lookup.insert(id, name.into());
				}
			}
		}
	}

	// Collect IDs we still need to fetch
	let mut missing: Vec<u32> = Vec::new();
	for w in works {
		for gid in &w.genre_ids {
			if !lookup.contains_key(gid) && !missing.contains(gid) {
				missing.push(*gid);
			}
		}
	}

	if missing.is_empty() {
		return lookup;
	}

	// Fetch missing genres
	if let Ok(genres) = play::get_genres(&missing) {
		for g in &genres {
			if let Some(ref name) = g.name {
				lookup.insert(g.id, name.best());
			}
		}

		// Persist updated cache
		let cache_str: String = lookup
			.iter()
			.map(|(id, name)| format!("{}:{}", id, name))
			.collect::<Vec<_>>()
			.join("\n");
		settings::set_cached_genres(&cache_str);
	}

	lookup
}

/// Build a series title_id → name lookup from the top-level series array.
fn build_series_lookup(series: &[models::SeriesInfo]) -> BTreeMap<String, String> {
	let mut map = BTreeMap::new();
	for s in series {
		map.insert(s.title_id.clone(), s.name.clone());
	}
	map
}

/// Sort works for volume ordering: by `volume_number` if present, then by
/// `regist_date` as fallback. Works with a volume_number sort before those
/// without.
fn sort_works_by_volume(works: &mut [models::PurchaseWork]) {
	works.sort_by(|a, b| {
		let a_vol = a.series.as_ref().and_then(|s| s.volume_number);
		let b_vol = b.series.as_ref().and_then(|s| s.volume_number);
		match (a_vol, b_vol) {
			(Some(av), Some(bv)) => av.cmp(&bv),
			(Some(_), None) => core::cmp::Ordering::Less,
			(None, Some(_)) => core::cmp::Ordering::Greater,
			(None, None) => {
				let a_date = a.regist_date.as_deref().unwrap_or("");
				let b_date = b.regist_date.as_deref().unwrap_or("");
				a_date.cmp(b_date)
			}
		}
	});
}

/// Check whether a work passes the given filters.
fn work_passes_filter(
	w: &models::PurchaseWork,
	q_lower: Option<&str>,
	q_raw: Option<&str>,
	work_types: &[String],
	work_type_exclude: &[String],
	translation_filter: Option<&str>,
	genre_filter: &[u32],
	genre_exclude: &[u32],
	content_rating_filter: &[String],
	series_name: Option<&str>,
) -> bool {
	if let Some(q) = q_lower {
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
		let raw = q_raw.unwrap_or_default();
		let sname = series_name
			.map(|s| s.to_lowercase())
			.unwrap_or_default();
		if !(name.contains(q)
			|| maker.contains(q)
			|| w.workno.contains(raw)
			|| sname.contains(q))
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
	if !work_type_exclude.is_empty() {
		let wt = w.work_type.as_deref().unwrap_or("");
		if work_type_exclude.iter().any(|t| t == wt) {
			return false;
		}
	}
	if let Some(tf) = translation_filter {
		let is_translated = w.has_translator();
		match tf {
			"translated" if !is_translated => return false,
			"original" if is_translated => return false,
			_ => {}
		}
	}
	if !genre_filter.is_empty() {
		let has_genre = genre_filter
			.iter()
			.all(|gid| w.genre_ids.contains(gid));
		if !has_genre {
			return false;
		}
	}
	if !genre_exclude.is_empty() {
		if genre_exclude.iter().any(|gid| w.genre_ids.contains(gid)) {
			return false;
		}
	}
	if !w.matches_content_ratings(content_rating_filter) {
		return false;
	}
	true
}

/// Groups works by series, applies filters, sorts, and returns sorted entries.
///
/// This is the core processing logic shared by `get_manga_list_inner` and
/// `get_home`. Accepts pre-fetched data so callers can reuse a single API
/// response for multiple purposes (e.g. carousel + library preview).
fn build_sorted_entries(
	worknos: &[String],
	resp: models::WorksResponse,
	query: Option<String>,
	work_types: Vec<String>,
	work_type_exclude: Vec<String>,
	translation_filter: Option<String>,
	genre_filter: Vec<u32>,
	genre_exclude: Vec<u32>,
	content_rating_filter: Vec<String>,
	sort_option: SortOption,
	sort_ascending: bool,
) -> Vec<(SortKey, Manga)> {
	let work_refs: Vec<&models::PurchaseWork> = resp.works.iter().collect();
	let genre_names = resolve_genre_names(&work_refs);
	let series_names = build_series_lookup(&resp.series);

	print(format!(
		"[dlsite-play] build_sorted_entries sort={} ascending={}",
		sort_option as i32, sort_ascending
	));

	// Fetch view histories for "recently opened" sort.
	let view_history_map: BTreeMap<String, String> = if sort_option == SortOption::RecentlyOpened {
		play::get_view_histories()
			.unwrap_or_default()
			.into_iter()
			.filter_map(|e| {
				let at = e.accessed_at?;
				Some((e.workno, at))
			})
			.collect()
	} else {
		BTreeMap::new()
	};

	// --- Group works by series title_id ---
	// Insertion order: first occurrence of each title_id / standalone work
	// determines the group's position in the final list.
	let mut group_order: Vec<String> = Vec::new(); // keys in display order
	let mut series_groups: BTreeMap<String, Vec<models::PurchaseWork>> = BTreeMap::new();
	let mut standalone: Vec<(String, models::PurchaseWork)> = Vec::new();

	for w in resp.works {
		if let Some(ref ws) = w.series {
			let tid = ws.title_id.clone();
			if !group_order.contains(&tid) {
				group_order.push(tid.clone());
			}
			series_groups.entry(tid).or_default().push(w);
		} else {
			let key = w.workno.clone();
			if !group_order.contains(&key) {
				group_order.push(key.clone());
			}
			standalone.push((key, w));
		}
	}

	// Sort each series group by volume order
	for works in series_groups.values_mut() {
		sort_works_by_volume(works);
	}

	// Cache series mappings for get_manga_update
	let mut cached_title_ids: Vec<String> = Vec::new();
	for (tid, works) in &series_groups {
		let wids: Vec<String> = works.iter().map(|w| w.workno.clone()).collect();
		settings::set_cached_series_map(tid, &wids);
		cached_title_ids.push(tid.clone());
	}
	settings::set_cached_series_title_ids(&cached_title_ids);

	// --- Build Manga entries with sort keys, applying filters ---
	let q_lower = query.as_ref().map(|q| q.to_lowercase());
	let has_filter = query.is_some()
		|| !work_types.is_empty()
		|| !work_type_exclude.is_empty()
		|| translation_filter.is_some()
		|| !genre_filter.is_empty()
		|| !genre_exclude.is_empty()
		|| !content_rating_filter.is_empty();

	let mut all_entries: Vec<(SortKey, Manga)> = Vec::new();

	for key in &group_order {
		if let Some(works) = series_groups.get(key.as_str()) {
			// Series group — match if ANY work passes filters
			let derived;
			let sname = match series_names.get(key.as_str()) {
				Some(s) => Some(s.as_str()),
				None => {
					derived = models::derive_series_name(works);
					derived.as_deref()
				}
			};
			if has_filter {
				let any_match = works.iter().any(|w| {
					work_passes_filter(
						w,
						q_lower.as_deref(),
						query.as_deref(),
						&work_types,
						&work_type_exclude,
						translation_filter.as_deref(),
						&genre_filter,
						&genre_exclude,
						&content_rating_filter,
						sname,
					)
				});
				if !any_match {
					continue;
				}
			}
			let name = sname.unwrap_or(key.as_str());
			let sort_key = build_series_sort_key(works, name, worknos, &view_history_map);
			all_entries.push((sort_key, models::series_manga(key, name, works, &genre_names)));
		} else if let Some(pos) = standalone.iter().position(|(k, _)| k == key) {
			let (_, w) = &standalone[pos];
			if has_filter {
				if !work_passes_filter(
					w,
					q_lower.as_deref(),
					query.as_deref(),
					&work_types,
					&work_type_exclude,
					translation_filter.as_deref(),
					&genre_filter,
					&genre_exclude,
					&content_rating_filter,
					None,
				) {
					continue;
				}
			}
			let sort_key = build_work_sort_key(w, worknos, &view_history_map);
			// Move out of standalone to convert
			let (_, w) = standalone.remove(pos);
			all_entries.push((sort_key, w.into_manga(&genre_names, &series_names)));
		}
	}

	// --- Sort ---
	apply_sort(&mut all_entries, sort_option, sort_ascending);

	all_entries
}

/// Core listing/search implementation shared by search and listing providers.
///
/// Fetches all works, groups them by series `title_id`, and returns paginated
/// results where each series is a single Manga entry.
fn get_manga_list_inner(
	query: Option<String>,
	page: i32,
	work_types: Vec<String>,
	work_type_exclude: Vec<String>,
	translation_filter: Option<String>,
	genre_filter: Vec<u32>,
	genre_exclude: Vec<u32>,
	content_rating_filter: Vec<String>,
	sort_option: SortOption,
	sort_ascending: bool,
) -> Result<MangaPageResult> {
	let worknos = get_or_fetch_worknos(page)?;

	if worknos.is_empty() {
		return Ok(MangaPageResult {
			entries: Vec::new(),
			has_next_page: false,
		});
	}

	let resp = play::get_works(&worknos)?;
	let all_entries = build_sorted_entries(
		&worknos, resp, query, work_types, work_type_exclude, translation_filter,
		genre_filter, genre_exclude, content_rating_filter, sort_option, sort_ascending,
	);

	// --- Paginate ---
	let page_idx = (page - 1).max(0) as usize;
	let start = page_idx * PAGE_SIZE;
	let total = all_entries.len();

	if start >= total {
		return Ok(MangaPageResult {
			entries: Vec::new(),
			has_next_page: false,
		});
	}

	let end = (start + PAGE_SIZE).min(total);
	let entries: Vec<Manga> = all_entries
		.into_iter()
		.skip(start)
		.take(end - start)
		.map(|(_, manga)| manga)
		.collect();

	Ok(MangaPageResult {
		entries,
		has_next_page: end < total,
	})
}

/// Build a `SortKey` for a standalone work.
fn build_work_sort_key(
	w: &models::PurchaseWork,
	worknos: &[String],
	view_history: &BTreeMap<String, String>,
) -> SortKey {
	SortKey {
		original_position: worknos.iter().position(|id| id == &w.workno).unwrap_or(usize::MAX),
		recently_opened: view_history.get(&w.workno).cloned().unwrap_or_default(),
		release_date: w.regist_date.clone().unwrap_or_default(),
		writer_name: w
			.maker
			.as_ref()
			.and_then(|m| m.name.as_ref())
			.map(|n| n.best().to_lowercase())
			.unwrap_or_default(),
		title: w
			.name
			.as_ref()
			.map(|n| n.best().to_lowercase())
			.unwrap_or_default(),
	}
}

/// Build a `SortKey` for a series group.
fn build_series_sort_key(
	works: &[models::PurchaseWork],
	series_name: &str,
	worknos: &[String],
	view_history: &BTreeMap<String, String>,
) -> SortKey {
	SortKey {
		original_position: works
			.iter()
			.filter_map(|w| worknos.iter().position(|id| id == &w.workno))
			.min()
			.unwrap_or(usize::MAX),
		recently_opened: works
			.iter()
			.filter_map(|w| view_history.get(&w.workno))
			.max()
			.cloned()
			.unwrap_or_default(),
		release_date: works
			.iter()
			.filter_map(|w| w.regist_date.as_deref())
			.min()
			.unwrap_or("")
			.into(),
		writer_name: works
			.first()
			.and_then(|w| w.maker.as_ref())
			.and_then(|m| m.name.as_ref())
			.map(|n| n.best().to_lowercase())
			.unwrap_or_default(),
		title: series_name.to_lowercase(),
	}
}

/// Sort entries by the selected sort option and direction.
fn apply_sort(entries: &mut Vec<(SortKey, Manga)>, sort_option: SortOption, ascending: bool) {
	entries.sort_by(|(a, _), (b, _)| {
		let ord = match sort_option {
			SortOption::RecentlyOpened => {
				// Works with view history sort by accessed_at.
				// Works without fall back to purchase order (lower position = newer).
				match (a.recently_opened.is_empty(), b.recently_opened.is_empty()) {
					(false, false) => a.recently_opened.cmp(&b.recently_opened),
					(false, true) => core::cmp::Ordering::Greater,
					(true, false) => core::cmp::Ordering::Less,
					(true, true) => b.original_position.cmp(&a.original_position),
				}
			}
			// worknos is sorted by purchase date descending (newest = 0),
			// so lower position = newer. Reverse comparison for ascending order.
			SortOption::PurchaseDate => b.original_position.cmp(&a.original_position),
			SortOption::ReleaseDate => a.release_date.cmp(&b.release_date),
			SortOption::WriterCircle => a.writer_name.cmp(&b.writer_name),
			SortOption::Title => a.title.cmp(&b.title),
		};
		if ascending { ord } else { ord.reverse() }
	});
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

		let sales = play::get_sales()?;
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
			let sales = play::get_sales()?;
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
	let editions = public::get_language_editions(workno).ok()?;
	if editions.is_empty() {
		return None;
	}
	let pairs: Vec<String> = editions
		.iter()
		.map(|e| {
			let name = lang_english_name(&e.lang);
			let name = if name == "Other" { &e.label } else { name };
			format!("{}:{}", e.lang, name)
		})
		.collect();
	let value = pairs.join(",");
	settings::set_cached_languages(workno, &value);
	Some(value)
}

/// Resolve the specific language of a purchased work by matching its workno
/// against the language editions list. Falls back to all edition labels when
/// the workno doesn't appear in the editions (e.g. mono-language works).
fn get_work_language(workno: &str) -> Option<String> {
	let editions = public::get_language_editions(workno).ok()?;
	if editions.is_empty() {
		return None;
	}
	// If the queried workno matches one of the editions, that's the language
	// of the copy the user owns.
	if let Some(ed) = editions.iter().find(|e| e.workno == workno) {
		let name = lang_english_name(&ed.lang);
		return Some(if name == "Other" {
			ed.label.clone()
		} else {
			name.to_string()
		});
	}
	// Mono-language works only have one edition — use it directly.
	if editions.len() == 1 {
		let ed = &editions[0];
		let name = lang_english_name(&ed.lang);
		return Some(if name == "Other" {
			ed.label.clone()
		} else {
			name.to_string()
		});
	}
	// Multiple editions but no workno match — list them all.
	let labels: Vec<&str> = editions
		.iter()
		.map(|e| {
			let name = lang_english_name(&e.lang);
			if name == "Other" { e.label.as_str() } else { name }
		})
		.collect();
	Some(labels.join(", "))
}

fn lang_english_name(code: &str) -> &'static str {
	match code {
		"JPN" => "Japanese",
		"ENG" => "English",
		"KO_KR" => "Korean",
		"CHI_HANS" => "Chinese (Simplified)",
		"CHI_HANT" => "Chinese (Traditional)",
		"SPA" => "Spanish",
		"ARA" => "Arabic",
		"GER" => "German",
		"FRE" => "French",
		"IND" => "Indonesian",
		"ITA" => "Italian",
		"POR" => "Portuguese",
		"SWE" => "Swedish",
		"THA" => "Thai",
		"VIE" => "Vietnamese",
		_ => "Other",
	}
}

/// Batch-fetch language editions for all works in the library.
/// Respects rate limits and skips already-cached works.
fn fetch_all_languages() {
	let worknos = settings::get_cached_worknos();
	if worknos.is_empty() {
		return;
	}

	set_rate_limit(3, 1, TimeUnit::Seconds);

	let total = worknos.len();
	for (i, workno) in worknos.iter().enumerate() {
		print(format!(
			"[dlsite-play] Fetching languages ({}/{}): {}",
			i + 1,
			total,
			workno
		));
		let _ = get_or_fetch_languages(workno);
	}

	print(format!(
		"[dlsite-play] Language fetch complete ({} works processed)",
		total
	));
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

fn extract_genre_filter(filters: &[FilterValue]) -> (Vec<u32>, Vec<u32>) {
	let mut inc = Vec::new();
	let mut exc = Vec::new();
	for f in filters {
		if let FilterValue::MultiSelect { id, included, excluded, .. } = f {
			if id.starts_with("genre_") {
				inc.extend(included.iter().filter_map(|s| s.parse::<u32>().ok()));
				exc.extend(excluded.iter().filter_map(|s| s.parse::<u32>().ok()));
			}
		}
	}
	(inc, exc)
}

fn extract_work_type_filter(filters: &[FilterValue]) -> (Vec<String>, Vec<String>) {
	for f in filters {
		if let FilterValue::MultiSelect { id, included, excluded, .. } = f {
			if id == "work_type" && (!included.is_empty() || !excluded.is_empty()) {
				return (included.clone(), excluded.clone());
			}
		}
	}
	(Vec::new(), Vec::new())
}

/// Extract sort option from filters. Returns `(SortOption, ascending)`.
/// Falls back to settings defaults if no sort filter is present.
fn extract_sort_filter(filters: &[FilterValue]) -> (SortOption, bool) {
	for f in filters {
		if let FilterValue::Sort { index, ascending, .. } = f {
			return (SortOption::from_index(*index), *ascending);
		}
	}
	(settings::get_default_sort(), settings::get_default_sort_ascending())
}

fn extract_content_rating_filter(filters: &[FilterValue]) -> Vec<String> {
	for f in filters {
		if let FilterValue::MultiSelect { id, included, .. } = f {
			if id == "content_rating" && !included.is_empty() {
				return included.clone();
			}
		}
	}
	Vec::new()
}

/// Read content rating filter from settings.
fn settings_content_rating_to_filter() -> Vec<String> {
	settings::get_default_content_ratings()
}

register_source!(
	DlsitePlay,
	ListingProvider,
	Home,
	WebLoginHandler,
	NotificationHandler,
	ImageRequestProvider,
	PageImageProcessor
);

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku::alloc::{string::ToString, vec};
	use aidoku_test::aidoku_test;

	// -- split_series_chapter_key tests --

	#[aidoku_test]
	fn split_series_chapter_key_standard() {
		let (workno, key) = split_series_chapter_key("RJ274802:img:root");
		assert_eq!(workno, "RJ274802");
		assert_eq!(key, "img:root");
	}

	#[aidoku_test]
	fn split_series_chapter_key_no_colon() {
		let (workno, key) = split_series_chapter_key("RJ274802");
		assert_eq!(workno, "RJ274802");
		assert_eq!(key, "");
	}

	#[aidoku_test]
	fn split_series_chapter_key_multiple_colons() {
		let (workno, key) = split_series_chapter_key("BJ295623:pdf:path/to/file.pdf#0001");
		assert_eq!(workno, "BJ295623");
		assert_eq!(key, "pdf:path/to/file.pdf#0001");
	}

	// -- percent_encode_path tests --

	#[aidoku_test]
	fn percent_encode_unreserved_chars() {
		assert_eq!(percent_encode_path("abc123"), "abc123");
		assert_eq!(percent_encode_path("file-name_v2.txt"), "file-name_v2.txt");
	}

	#[aidoku_test]
	fn percent_encode_spaces_and_special() {
		assert_eq!(percent_encode_path("hello world"), "hello%20world");
		assert_eq!(percent_encode_path("a/b"), "a%2Fb");
		assert_eq!(percent_encode_path("a+b"), "a%2Bb");
	}

	#[aidoku_test]
	fn percent_encode_empty() {
		assert_eq!(percent_encode_path(""), "");
	}

	// -- play_viewer_url tests --

	#[aidoku_test]
	fn play_viewer_url_simple() {
		let url = play_viewer_url("RJ274802", "images/page1.jpg");
		assert_eq!(url, "https://play.dlsite.com/work/RJ274802/view/images%2Fpage1.jpg");
	}

	// -- sort_works_by_volume tests --

	#[aidoku_test]
	fn sort_works_by_volume_number() {
		let mut works = vec![
			make_work_with_volume("RJ003", Some(3), None),
			make_work_with_volume("RJ001", Some(1), None),
			make_work_with_volume("RJ002", Some(2), None),
		];
		sort_works_by_volume(&mut works);
		assert_eq!(works[0].workno, "RJ001");
		assert_eq!(works[1].workno, "RJ002");
		assert_eq!(works[2].workno, "RJ003");
	}

	#[aidoku_test]
	fn sort_works_by_volume_with_fallback_to_date() {
		let mut works = vec![
			make_work_with_volume("RJ002", None, Some("2024-02-01")),
			make_work_with_volume("RJ001", None, Some("2024-01-01")),
		];
		sort_works_by_volume(&mut works);
		assert_eq!(works[0].workno, "RJ001");
		assert_eq!(works[1].workno, "RJ002");
	}

	#[aidoku_test]
	fn sort_works_volume_number_before_none() {
		let mut works = vec![
			make_work_with_volume("RJ_no_vol", None, Some("2020-01-01")),
			make_work_with_volume("RJ_vol1", Some(1), None),
		];
		sort_works_by_volume(&mut works);
		assert_eq!(works[0].workno, "RJ_vol1");
		assert_eq!(works[1].workno, "RJ_no_vol");
	}

	// -- work_passes_filter tests --

	#[aidoku_test]
	fn work_passes_filter_no_filters() {
		let w = make_filter_work("RJ001", "Test Work", "MNG", false);
		assert!(work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &[], None));
	}

	#[aidoku_test]
	fn work_passes_filter_query_match_name() {
		let w = make_filter_work("RJ001", "My Great Manga", "MNG", false);
		assert!(work_passes_filter(
			&w, Some("great"), Some("great"), &[], &[], None, &[], &[], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_query_no_match() {
		let w = make_filter_work("RJ001", "My Manga", "MNG", false);
		assert!(!work_passes_filter(
			&w, Some("zzzzz"), Some("zzzzz"), &[], &[], None, &[], &[], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_query_match_workno() {
		let w = make_filter_work("RJ123456", "Title", "MNG", false);
		assert!(work_passes_filter(
			&w, Some("rj123456"), Some("RJ123456"), &[], &[], None, &[], &[], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_work_type() {
		let w = make_filter_work("RJ001", "Test", "MNG", false);
		assert!(work_passes_filter(
			&w, None, None, &["MNG".to_string()], &[], None, &[], &[], &[], None
		));
		assert!(!work_passes_filter(
			&w, None, None, &["CG".to_string()], &[], None, &[], &[], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_translation() {
		let w_no_trans = make_filter_work("RJ001", "Test", "MNG", false);
		let w_trans = make_filter_work("RJ002", "Test", "MNG", true);

		assert!(work_passes_filter(
			&w_trans, None, None, &[], &[], Some("translated"), &[], &[], &[], None
		));
		assert!(!work_passes_filter(
			&w_no_trans, None, None, &[], &[], Some("translated"), &[], &[], &[], None
		));
		assert!(work_passes_filter(
			&w_no_trans, None, None, &[], &[], Some("original"), &[], &[], &[], None
		));
		assert!(!work_passes_filter(
			&w_trans, None, None, &[], &[], Some("original"), &[], &[], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_genre() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.genre_ids = vec![100, 200];

		// Work has genre 100 — filter for 100 should pass
		assert!(work_passes_filter(
			&w, None, None, &[], &[], None, &[100], &[], &[], None
		));
		// Work does not have genre 300 — filter for 300 should fail
		assert!(!work_passes_filter(
			&w, None, None, &[], &[], None, &[300], &[], &[], None
		));
		// Filter for both 100 and 200 — work has both, should pass
		assert!(work_passes_filter(
			&w, None, None, &[], &[], None, &[100, 200], &[], &[], None
		));
		// Filter for 100 and 300 — work missing 300, should fail
		assert!(!work_passes_filter(
			&w, None, None, &[], &[], None, &[100, 300], &[], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_genre_exclude() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.genre_ids = vec![100, 200];

		// Excluding genre 100 — work has it, should fail
		assert!(!work_passes_filter(
			&w, None, None, &[], &[], None, &[], &[100], &[], None
		));
		// Excluding genre 300 — work doesn't have it, should pass
		assert!(work_passes_filter(
			&w, None, None, &[], &[], None, &[], &[300], &[], None
		));
		// Excluding 300 and 400 — work has neither, should pass
		assert!(work_passes_filter(
			&w, None, None, &[], &[], None, &[], &[300, 400], &[], None
		));
		// Excluding 200 and 300 — work has 200, should fail
		assert!(!work_passes_filter(
			&w, None, None, &[], &[], None, &[], &[200, 300], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_work_type_exclude() {
		let w = make_filter_work("RJ001", "Test", "MNG", false);

		// Excluding MNG — work is MNG, should fail
		assert!(!work_passes_filter(
			&w, None, None, &[], &["MNG".to_string()], None, &[], &[], &[], None
		));
		// Excluding CG — work is MNG, should pass
		assert!(work_passes_filter(
			&w, None, None, &[], &["CG".to_string()], None, &[], &[], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_genre_include_and_exclude() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.genre_ids = vec![100, 200, 300];

		// Include 100, exclude 300 — work has both, exclude wins → fail
		assert!(!work_passes_filter(
			&w, None, None, &[], &[], None, &[100], &[300], &[], None
		));
		// Include 100, exclude 400 — work passes include and no excluded match → pass
		assert!(work_passes_filter(
			&w, None, None, &[], &[], None, &[100], &[400], &[], None
		));
	}

	#[aidoku_test]
	fn work_passes_filter_work_type_include_and_exclude_disjoint() {
		let w = make_filter_work("RJ001", "Test", "MNG", false);

		// Include MNG, exclude CG — work is MNG, not CG → pass
		assert!(work_passes_filter(
			&w, None, None, &["MNG".to_string()], &["CG".to_string()], None, &[], &[], &[], None
		));
		// Include MNG, exclude MNG — exclude wins → fail
		assert!(!work_passes_filter(
			&w, None, None, &["MNG".to_string()], &["MNG".to_string()], None, &[], &[], &[], None
		));
	}

	// -- content rating filter tests --

	#[aidoku_test]
	fn work_passes_filter_content_rating_safe() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.age_category = None;
		assert!(work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["safe".into()], None));
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["r18".into()], None));
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["r15".into()], None));
	}

	#[aidoku_test]
	fn work_passes_filter_content_rating_r18() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.age_category = Some("R18".into());
		assert!(work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["r18".into()], None));
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["safe".into()], None));
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["r15".into()], None));
	}

	#[aidoku_test]
	fn work_passes_filter_content_rating_r15() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.age_category = Some("R15".into());
		assert!(work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["r15".into()], None));
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["safe".into()], None));
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["r18".into()], None));
	}

	#[aidoku_test]
	fn work_passes_filter_content_rating_all() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.age_category = Some("R18".into());
		// Empty slice means "All" / no filter
		assert!(work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &[], None));
	}

	#[aidoku_test]
	fn work_passes_filter_content_rating_case_insensitive() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.age_category = Some("r18".into()); // lowercase from API
		assert!(work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["r18".into()], None));
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["safe".into()], None));
	}

	#[aidoku_test]
	fn work_passes_filter_content_rating_multi() {
		let mut w = make_filter_work("RJ001", "Test", "MNG", false);
		w.age_category = Some("R15".into());
		// Safe + R-15 selected: R-15 work matches
		assert!(work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["safe".into(), "r15".into()], None));
		// Safe + R-18 selected: R-15 work does not match
		assert!(!work_passes_filter(&w, None, None, &[], &[], None, &[], &[], &["safe".into(), "r18".into()], None));
	}

	// -- test helpers --

	fn make_work_with_volume(
		workno: &str,
		volume: Option<u32>,
		regist_date: Option<&str>,
	) -> models::PurchaseWork {
		models::PurchaseWork {
			workno: workno.into(),
			name: None,
			name_phonetic: None,
			maker: None,
			translator: None,
			author_name: None,
			work_type: None,
			file_type: None,
			age_category: None,
			dl_format: None,
			site_id: None,
			content_length: None,
			content_count: None,
			content_size: None,
			touch_content_count: None,
			touch_site_id: None,
			os: None,
			work_files: None,
			is_playwork: None,
			downloadable: None,
			encodable: None,
			app_type: None,
			viewer_type: None,
			tags: None,
			regist_date: regist_date.map(|s| s.into()),
			upgrade_date: None,
			sales_date: None,
			genre_ids: Vec::new(),
			series: volume.map(|v| models::WorkSeries {
				title_id: "S001".into(),
				volume_number: Some(v),
			}),
			purchase_type: None,
			download_start_date: None,
		}
	}

	fn make_filter_work(
		workno: &str,
		title: &str,
		work_type: &str,
		has_translator: bool,
	) -> models::PurchaseWork {
		models::PurchaseWork {
			workno: workno.into(),
			name: Some(models::LocalizedName {
				ja_JP: None,
				en_US: Some(title.into()),
				zh_CN: None,
				zh_TW: None,
				ko_KR: None,
				es_ES: None,
				de_DE: None,
				fr_FR: None,
				it_IT: None,
				pt_BR: None,
				id_ID: None,
				vi_VN: None,
				th_TH: None,
				sv_SE: None,
			}),
			name_phonetic: None,
			maker: None,
			translator: if has_translator {
				Some(models::TranslatorInfo {
					id: "t1".into(),
					name: None,
					name_phonetic: None,
				})
			} else {
				None
			},
			author_name: None,
			work_type: Some(work_type.into()),
			file_type: None,
			age_category: None,
			dl_format: None,
			site_id: None,
			content_length: None,
			content_count: None,
			content_size: None,
			touch_content_count: None,
			touch_site_id: None,
			os: None,
			work_files: None,
			is_playwork: None,
			downloadable: None,
			encodable: None,
			app_type: None,
			viewer_type: None,
			tags: None,
			regist_date: None,
			upgrade_date: None,
			sales_date: None,
			genre_ids: Vec::new(),
			series: None,
			purchase_type: None,
			download_start_date: None,
		}
	}
}
