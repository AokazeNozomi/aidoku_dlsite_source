#![no_std]

use aidoku::{
	alloc::{String, Vec},
	register_source, FilterValue, Manga, MangaPageResult, Page, Result, Source,
};

use dlsite_common::{explore, filters, settings};

const SITE_SLUGS: &[&str] = &["maniax", "pro", "books", "girls", "bl"];

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
		let sort = filters::extract_sort_filter(&filter_list);
		let language = filters::extract_language_filter(&filter_list);
		let work_types = filters::extract_work_type_filter(&filter_list);
		let content_rating_filter = filters::extract_content_rating_filter(&filter_list);
		let genre_filter = filters::extract_genre_filter(&filter_list);

		let result = explore::search_explore(
			site_slug,
			query.as_deref(),
			page,
			sort,
			&language,
			&work_types,
			&content_rating_filter,
			&genre_filter,
		)?;

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

register_source!(DlsiteExploreR18);
