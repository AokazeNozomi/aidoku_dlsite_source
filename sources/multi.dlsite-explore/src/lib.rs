#![no_std]

use aidoku::{
	alloc::{String, Vec},
	register_source, FilterValue, Manga, MangaPageResult, Page, Result, Source,
};

use dlsite_common::{explore, filters, settings};

const DEFAULT_SITE_SLUG: &str = "home";

struct DlsiteExplore;

impl Source for DlsiteExplore {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filter_list: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let site_slug = settings::get_site_slug(DEFAULT_SITE_SLUG);
		let sort = filters::extract_sort_filter(&filter_list);
		let language = filters::extract_language_filter(&filter_list);
		let work_types = filters::extract_work_type_filter(&filter_list);
		let content_rating_filter = filters::extract_content_rating_filter(&filter_list);
		let genre_filter = filters::extract_genre_filter(&filter_list);

		// Fall back to settings work types if no filter selected
		let effective_work_types = if work_types.is_empty() {
			settings::get_work_type_setting()
		} else {
			work_types
		};

		// Fall back to settings content rating if no filter selected
		let effective_content_rating = if content_rating_filter.is_empty() {
			settings::get_default_content_ratings()
		} else {
			content_rating_filter
		};

		let result = explore::search_explore(
			site_slug,
			query.as_deref(),
			page,
			sort,
			&language,
			&effective_work_types,
			&effective_content_rating,
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
			let site_slug = settings::get_site_slug(DEFAULT_SITE_SLUG);
			let locale = settings::get_preferred_language().locale_code();
			if let Ok(Some(public_work)) =
				dlsite_common::api::get_public_work_details(site_slug, &manga.key, Some(locale))
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

register_source!(DlsiteExplore);

#[cfg(test)]
mod tests {
	use aidoku::alloc::vec;
	use aidoku_test::aidoku_test;
	use dlsite_common::explore::*;

	#[aidoku_test]
	fn cover_url_rj_product() {
		let url = cover_url_from_id("RJ01599911").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/doujin/RJ01600000/RJ01599911_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn cover_url_vj_product() {
		let url = cover_url_from_id("VJ01006082").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/professional/VJ01007000/VJ01006082_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn cover_url_bj_product() {
		let url = cover_url_from_id("BJ02452708").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/books/BJ02453000/BJ02452708_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn cover_url_exact_thousand() {
		let url = cover_url_from_id("RJ01000000").unwrap();
		assert_eq!(
			url,
			"https://img.dlsite.jp/resize/images2/work/doujin/RJ01000000/RJ01000000_img_main_240x240.jpg"
		);
	}

	#[aidoku_test]
	fn parse_age_adl() {
		assert_eq!(
			parse_age_from_attributes("RG45215,adl,male,ICG,JPN"),
			Some("adl".into())
		);
	}

	#[aidoku_test]
	fn parse_age_r15() {
		assert_eq!(
			parse_age_from_attributes("RG12345,r15,male,MNG,JPN"),
			Some("r15".into())
		);
	}

	#[aidoku_test]
	fn parse_age_general() {
		assert_eq!(
			parse_age_from_attributes("RG12345,general,male,SOU,JPN"),
			Some("general".into())
		);
	}

	#[aidoku_test]
	fn parse_age_empty() {
		assert_eq!(parse_age_from_attributes(""), None);
	}

	#[aidoku_test]
	fn build_search_url_basic() {
		let url = build_search_url("maniax", None, 1, ExploreSort::Newest, &[], &[], &[], &[]);
		assert_eq!(
			url,
			"https://www.dlsite.com/maniax/fsr/ajax/=/language/jp/order%5B0%5D/release_d/page/1"
		);
	}

	#[aidoku_test]
	fn build_search_url_different_site() {
		let url = build_search_url("home", None, 1, ExploreSort::Trending, &[], &[], &[], &[]);
		assert_eq!(
			url,
			"https://www.dlsite.com/home/fsr/ajax/=/language/jp/order%5B0%5D/trend/page/1"
		);
	}

	#[aidoku_test]
	fn build_search_url_with_single_language() {
		let langs = vec!["ENG".into()];
		let url = build_search_url("maniax", None, 1, ExploreSort::Newest, &langs, &[], &[], &[]);
		assert!(url.contains("/language/jp"));
		assert!(url.contains("/options_and_or/and"));
		assert!(url.contains("/options%5B0%5D/ENG"));
	}

	#[aidoku_test]
	fn build_search_url_with_multiple_languages() {
		let langs = vec!["JPN".into(), "ENG".into(), "NM".into()];
		let url = build_search_url("maniax", None, 1, ExploreSort::Newest, &langs, &[], &[], &[]);
		assert!(url.contains("/language/jp"));
		assert!(url.contains("/options_and_or/and"));
		assert!(url.contains("/options%5B0%5D/JPN"));
		assert!(url.contains("/options%5B1%5D/ENG"));
		assert!(url.contains("/options%5B2%5D/NM"));
	}

	#[aidoku_test]
	fn build_search_url_with_filters() {
		let types = vec!["MNG".into(), "WBT".into()];
		let ratings = vec!["r18".into()];
		let url = build_search_url(
			"maniax",
			Some("test"),
			2,
			ExploreSort::Trending,
			&[],
			&types,
			&ratings,
			&[],
		);
		assert!(url.contains("/age_category%5B0%5D/adult"));
		assert!(url.contains("/work_type%5B0%5D/MNG"));
		assert!(url.contains("/work_type%5B1%5D/WBT"));
		assert!(url.contains("/keyword/test"));
		assert!(url.contains("/order%5B0%5D/trend"));
		assert!(url.contains("/page/2"));
	}

	#[aidoku_test]
	fn build_search_url_with_multiple_ratings() {
		let ratings = vec!["safe".into(), "r15".into()];
		let url =
			build_search_url("maniax", None, 1, ExploreSort::Newest, &[], &[], &ratings, &[]);
		assert!(url.contains("/age_category%5B0%5D/general"));
		assert!(url.contains("/age_category%5B1%5D/r15"));
	}

	#[aidoku_test]
	fn build_search_url_with_genres() {
		let url =
			build_search_url("maniax", None, 1, ExploreSort::Newest, &[], &[], &[], &[509, 66]);
		assert!(url.contains("/genre%5B0%5D/509"));
		assert!(url.contains("/genre%5B1%5D/66"));
	}
}
