#![no_std]

use aidoku::{
	alloc::{String, Vec},
	register_source, FilterValue, Manga, MangaPageResult, Page, Result, Source,
};

mod api;
mod models;
mod settings;

use models::ExploreSort;

struct DlsiteExplore;

impl Source for DlsiteExplore {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let sort = extract_sort_filter(&filters);
		let language = extract_language_filter(&filters);
		let work_types = extract_work_type_filter(&filters);
		let content_rating_filter = extract_content_rating_filter(&filters);
		let genre_filter = extract_genre_filter(&filters);

		// Fall back to settings work types if no filter selected
		let effective_work_types = if work_types.is_empty() {
			settings::get_work_type_setting()
		} else {
			work_types
		};

		// Fall back to settings content rating if no filter selected
		let effective_content_rating = if content_rating_filter.is_empty() {
			settings_content_rating_to_filter()
		} else {
			content_rating_filter
		};

		let result = api::search_explore(
			query.as_deref(),
			page,
			sort,
			&language,
			&effective_work_types,
			&effective_content_rating,
			&genre_filter,
		)?;

		Ok(MangaPageResult {
			entries: result.works.into_iter().map(|w| w.into_manga()).collect(),
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
			let locale = settings::get_preferred_language().locale_code();
			if let Ok(Some(public_work)) = api::get_public_work_details(&manga.key, Some(locale)) {
				let updated = public_work.into_manga();
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

// ---------------------------------------------------------------------------
// Filter extraction
// ---------------------------------------------------------------------------

fn extract_language_filter(filters: &[FilterValue]) -> Vec<String> {
	for f in filters {
		if let FilterValue::MultiSelect { id, included, .. } = f {
			if id == "language" && !included.is_empty() {
				return included.clone();
			}
		}
	}
	Vec::new()
}

fn extract_sort_filter(filters: &[FilterValue]) -> ExploreSort {
	for f in filters {
		if let FilterValue::Sort { index, .. } = f {
			return ExploreSort::from_index(*index);
		}
	}
	ExploreSort::Trending
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

fn extract_genre_filter(filters: &[FilterValue]) -> Vec<u32> {
	let mut ids = Vec::new();
	for f in filters {
		if let FilterValue::MultiSelect { id, included, .. } = f {
			if id.starts_with("genre_") {
				ids.extend(included.iter().filter_map(|s| s.parse::<u32>().ok()));
			}
		}
	}
	ids
}

/// Read content rating filter from settings.
fn settings_content_rating_to_filter() -> Vec<String> {
	settings::get_default_content_ratings()
}

register_source!(DlsiteExplore);
