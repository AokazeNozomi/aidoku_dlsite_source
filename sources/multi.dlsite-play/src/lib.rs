#![no_std]

use aidoku::{
	alloc::{collections::BTreeMap, format, string::ToString, vec, String, Vec},
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
		let genre_filter = extract_genre_filter(&filters);
		get_manga_list_inner(query, page, work_types, translation_filter, genre_filter)
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

		let token = api::download_token(workno)?;
		let ziptree = api::fetch_ziptree(&token)?;
		let chapter_groups = helpers::extract_chapter_groups(&ziptree);
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
		}));

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

		if needs_details || needs_chapters {
			let resp = api::get_works(&[manga.key.clone()])?;
			let series = resp.series;
			if let Some(work) = resp.works.into_iter().next() {
				release_date = work.release_date_timestamp();
				if needs_details {
					let genre_names = resolve_genre_names(&[&work]);
					let series_names = build_series_lookup(&series);
					let updated = work.into_manga(&genre_names, &series_names);
					manga.copy_from(updated);

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

		let resp = api::get_works(&member_worknos)?;
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

				let token = api::download_token(&work.workno)?;
				let ziptree = api::fetch_ziptree(&token)?;
				let chapter_groups = helpers::extract_chapter_groups(&ziptree);
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
		let work_types = match listing.id.as_str() {
			"purchases" => Vec::new(),
			wt => vec![wt.to_string()],
		};
		get_manga_list_inner(None, page, work_types, None, None)
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
	let resp = api::get_works(&[workno.into()]).ok()?;
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
	if let Ok(genres) = api::get_genres(&missing) {
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
	translation_filter: Option<&str>,
	genre_filter_lower: Option<&str>,
	genre_names: &BTreeMap<u32, String>,
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
	if let Some(tf) = translation_filter {
		let is_translated = w.has_translator();
		match tf {
			"translated" if !is_translated => return false,
			"original" if is_translated => return false,
			_ => {}
		}
	}
	if let Some(gf) = genre_filter_lower {
		let has_genre = w.genre_ids.iter().any(|gid| {
			genre_names
				.get(gid)
				.map(|name| name.to_lowercase().contains(gf))
				.unwrap_or(false)
		});
		if !has_genre {
			return false;
		}
	}
	true
}

/// Core listing/search implementation shared by search and listing providers.
///
/// Fetches all works, groups them by series `title_id`, and returns paginated
/// results where each series is a single Manga entry.
fn get_manga_list_inner(
	query: Option<String>,
	page: i32,
	work_types: Vec<String>,
	translation_filter: Option<String>,
	genre_filter: Option<String>,
) -> Result<MangaPageResult> {
	let worknos = get_or_fetch_worknos(page)?;

	if worknos.is_empty() {
		return Ok(MangaPageResult {
			entries: Vec::new(),
			has_next_page: false,
		});
	}

	// Always fetch all works to enable cross-page series grouping.
	let resp = api::get_works(&worknos)?;

	let work_refs: Vec<&models::PurchaseWork> = resp.works.iter().collect();
	let genre_names = resolve_genre_names(&work_refs);
	let series_names = build_series_lookup(&resp.series);

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

	// --- Build Manga entries in display order, applying filters ---
	let q_lower = query.as_ref().map(|q| q.to_lowercase());
	let genre_filter_lower = genre_filter.as_ref().map(|g| g.to_lowercase());
	let has_filter = query.is_some()
		|| !work_types.is_empty()
		|| translation_filter.is_some()
		|| genre_filter.is_some();

	let mut all_entries: Vec<Manga> = Vec::new();

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
						translation_filter.as_deref(),
						genre_filter_lower.as_deref(),
						&genre_names,
						sname,
					)
				});
				if !any_match {
					continue;
				}
			}
			let name = sname.unwrap_or(key.as_str());
			all_entries.push(models::series_manga(key, name, works, &genre_names));
		} else if let Some(pos) = standalone.iter().position(|(k, _)| k == key) {
			let (_, w) = &standalone[pos];
			if has_filter {
				if !work_passes_filter(
					w,
					q_lower.as_deref(),
					query.as_deref(),
					&work_types,
					translation_filter.as_deref(),
					genre_filter_lower.as_deref(),
					&genre_names,
					None,
				) {
					continue;
				}
			}
			// Move out of standalone to convert
			let (_, w) = standalone.remove(pos);
			all_entries.push(w.into_manga(&genre_names, &series_names));
		}
	}

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
		.collect();

	Ok(MangaPageResult {
		entries,
		has_next_page: end < total,
	})
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

fn extract_genre_filter(filters: &[FilterValue]) -> Option<String> {
	for f in filters {
		if let FilterValue::Text { id, value } = f {
			if id == "genre" && !value.is_empty() {
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
