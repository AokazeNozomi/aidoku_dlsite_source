#![no_std]

use aidoku::{
	alloc::{format, String, Vec},
	prelude::*,
	register_source, FilterValue, HashMap, Home, HomeLayout, Listing, ListingProvider,
	Manga, MangaPageResult, Page, Result, Source, WebLoginHandler,
};

use dlsite_common::{explore, filters, home, settings};

const SITE_SLUGS: &[&str] = &["maniax", "pro", "books", "girls", "bl"];
const IS_R18: bool = true;

struct DlsiteExploreR18;

impl Source for DlsiteExploreR18 {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filter_list: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let site_slug = filters::extract_site_filter(&filter_list, SITE_SLUGS);
		settings::sync_locale_cookie(site_slug);
		let sort = filters::extract_sort_filter(&filter_list);
		let language = filters::extract_language_filter(&filter_list);
		let work_types = filters::extract_work_type_filter(&filter_list);
		let content_rating_filter = filters::extract_content_rating_filter(&filter_list);
		let genre_filter = filters::extract_genre_filter(&filter_list);

		let result = explore::search_explore(&explore::ExploreSearchParams {
			site_slug,
			keyword: query.as_deref(),
			page,
			sort,
			languages: &language,
			work_types: &work_types,
			content_ratings: &content_rating_filter,
			genres: &genre_filter,
		})?;

		Ok(MangaPageResult {
			entries: result
				.works
				.into_iter()
				.map(|w| w.into_manga(site_slug))
				.collect(),
			has_next_page: result.has_next_page,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		_needs_chapters: bool,
	) -> Result<Manga> {
		if needs_details {
			let (site_slug, product_id) = explore::split_key(&manga.key, SITE_SLUGS[0]);
			let locale = settings::get_preferred_language().locale_code();
			if let Ok(Some(public_work)) =
				dlsite_common::api::get_public_work_details(site_slug, product_id, Some(locale))
			{
				let updated = public_work.into_manga(site_slug);
				manga.copy_from(updated);
			}
		}
		// No chapters — works are not purchased.
		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, _chapter: aidoku::Chapter) -> Result<Vec<Page>> {
		Ok(Vec::new())
	}
}

impl ListingProvider for DlsiteExploreR18 {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let site_slug = settings::get_site_slug(SITE_SLUGS[0]);
		settings::sync_locale_cookie(site_slug);
		let work_types = settings::get_work_type_setting();
		let languages = settings::get_selected_languages();

		let result = match listing.id.as_str() {
			"english_picks" => home::fetch_english_picks(site_slug, IS_R18, page),
			"translations" => home::fetch_translations(site_slug, page),
			"ranking" => home::fetch_ranking(site_slug, &work_types),
			"recommended" => home::fetch_recommended(site_slug, "top"),
			"recommended_en" => home::fetch_recommended(site_slug, "top_en"),
			"recommended_discount" => home::fetch_recommended(site_slug, "top_discount"),
			"new_works" => home::fetch_new_works(site_slug, IS_R18, &languages, page),
			"popular_works" => home::fetch_popular_works(site_slug, IS_R18, &languages, page),
			_ => {
				return Ok(MangaPageResult {
					entries: Vec::new(),
					has_next_page: false,
				})
			}
		}?;

		Ok(MangaPageResult {
			entries: result
				.works
				.into_iter()
				.map(|w| w.into_manga(site_slug))
				.collect(),
			has_next_page: result.has_next_page,
		})
	}
}

impl Home for DlsiteExploreR18 {
	fn get_home(&self) -> Result<HomeLayout> {
		let site_slug = settings::get_site_slug(SITE_SLUGS[0]);
		settings::sync_locale_cookie(site_slug);
		let work_types = settings::get_work_type_setting();
		let languages = settings::get_selected_languages();
		home::build_explore_home(site_slug, IS_R18, &work_types, &languages)
	}
}

impl WebLoginHandler for DlsiteExploreR18 {
	fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
		if key != "login" {
			dlsite_common::debug_print!("[dlsite-explore-r18] web login rejected invalid key `{key}`");
			bail!("Invalid login key: `{key}`");
		}

		let mut keys: Vec<&str> = cookies.keys().map(|s| s.as_str()).collect();
		keys.sort();
		let mut cookie_pairs: Vec<String> = Vec::new();
		for name in &keys {
			if let Some(value) = cookies.get(*name) {
				cookie_pairs.push(format!("{}={}", name, value));
			}
		}

		let has_session = !cookie_pairs.is_empty();
		settings::set_logged_in(has_session);

		if has_session {
			let cookie_header = cookie_pairs.join("; ");
			settings::set_web_cookies(&cookie_header);
		} else {
			settings::clear_web_cookies();
		}

		Ok(has_session)
	}
}

register_source!(DlsiteExploreR18, ListingProvider, Home, WebLoginHandler);
