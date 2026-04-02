#![no_std]

use aidoku::{
	alloc::{format, String, Vec},
	imports::std::print,
	prelude::*,
	register_source, FilterValue, HashMap, Home, HomeComponent, HomeComponentValue,
	HomeLayout, Link, Listing, ListingKind, ListingProvider, Manga, MangaPageResult,
	Page, Result, Source, WebLoginHandler,
};

use dlsite_common::{explore, filters, home, settings};

const SITE_SLUGS: &[&str] = &["home", "soft"];
const IS_R18: bool = false;

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

impl ListingProvider for DlsiteExplore {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let site_slug = settings::get_site_slug(SITE_SLUGS[0]);
		let work_types = settings::get_work_type_setting();
		let languages = get_home_languages();

		let result = match listing.id.as_str() {
			"english_picks" => home::fetch_english_picks(site_slug, IS_R18, page),
			"translations" => home::fetch_translations(site_slug, page),
			"ranking" => home::fetch_ranking(site_slug, &work_types),
			"recommended" => home::fetch_recommended(site_slug, &work_types),
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

impl Home for DlsiteExplore {
	fn get_home(&self) -> Result<HomeLayout> {
		let site_slug = settings::get_site_slug(SITE_SLUGS[0]);
		let work_types = settings::get_work_type_setting();
		let languages = get_home_languages();
		let mut components = Vec::new();

		// 1. Top English Picks (carousel, no expand)
		if let Ok(result) = home::fetch_english_picks(site_slug, IS_R18, 1) {
			if !result.works.is_empty() {
				components.push(HomeComponent {
					title: Some(String::from("Our Top English Picks")),
					subtitle: None,
					value: HomeComponentValue::Scroller {
						entries: result
							.works
							.into_iter()
							.map(|w| -> Link { w.into_manga(site_slug).into() })
							.collect(),
						listing: Some(Listing {
							id: String::from("english_picks"),
							name: String::from("Our Top English Picks"),
							kind: ListingKind::default(),
						}),
					},
				});
			}
		}

		// 2. Translators Unite (carousel with expand)
		if let Ok(result) = home::fetch_translations(site_slug, 1) {
			if !result.works.is_empty() {
				components.push(HomeComponent {
					title: Some(String::from("New Translators Unite Translations")),
					subtitle: None,
					value: HomeComponentValue::Scroller {
						entries: result
							.works
							.into_iter()
							.map(|w| -> Link { w.into_manga(site_slug).into() })
							.collect(),
						listing: Some(Listing {
							id: String::from("translations"),
							name: String::from("Translators Unite"),
							kind: ListingKind::default(),
						}),
					},
				});
			}
		}

		// 3. Doujin Ranking 7 Days (carousel with expand)
		if let Ok(result) = home::fetch_ranking(site_slug, &work_types) {
			if !result.works.is_empty() {
				components.push(HomeComponent {
					title: Some(String::from("Doujin Ranking (7 Days)")),
					subtitle: None,
					value: HomeComponentValue::Scroller {
						entries: result
							.works
							.into_iter()
							.map(|w| -> Link { w.into_manga(site_slug).into() })
							.collect(),
						listing: Some(Listing {
							id: String::from("ranking"),
							name: String::from("Doujin Ranking (7 Days)"),
							kind: ListingKind::default(),
						}),
					},
				});
			}
		}

		// 4. Recommended (carousel with expand, always fetched)
		if let Ok(result) = home::fetch_recommended(site_slug, &work_types) {
			if !result.works.is_empty() {
				components.push(HomeComponent {
					title: Some(String::from("Recommended doujin products for you")),
					subtitle: None,
					value: HomeComponentValue::Scroller {
						entries: result
							.works
							.into_iter()
							.map(|w| -> Link { w.into_manga(site_slug).into() })
							.collect(),
						listing: Some(Listing {
							id: String::from("recommended"),
							name: String::from("Recommended"),
							kind: ListingKind::default(),
						}),
					},
				});
			}
		}

		// 5. New Works (carousel with expand)
		if let Ok(result) = home::fetch_new_works(site_slug, IS_R18, &languages, 1) {
			if !result.works.is_empty() {
				components.push(HomeComponent {
					title: Some(String::from("New doujin works")),
					subtitle: None,
					value: HomeComponentValue::Scroller {
						entries: result
							.works
							.into_iter()
							.map(|w| -> Link { w.into_manga(site_slug).into() })
							.collect(),
						listing: Some(Listing {
							id: String::from("new_works"),
							name: String::from("New Doujin Works"),
							kind: ListingKind::default(),
						}),
					},
				});
			}
		}

		// 6. Popular Works (carousel with expand)
		if let Ok(result) = home::fetch_popular_works(site_slug, IS_R18, &languages, 1) {
			if !result.works.is_empty() {
				components.push(HomeComponent {
					title: Some(String::from("Popular doujin works")),
					subtitle: None,
					value: HomeComponentValue::Scroller {
						entries: result
							.works
							.into_iter()
							.map(|w| -> Link { w.into_manga(site_slug).into() })
							.collect(),
						listing: Some(Listing {
							id: String::from("popular_works"),
							name: String::from("Popular Doujin Works"),
							kind: ListingKind::default(),
						}),
					},
				});
			}
		}

		Ok(HomeLayout { components })
	}
}

impl WebLoginHandler for DlsiteExplore {
	fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
		if key != "login" {
			print(format!(
				"[dlsite-explore] web login rejected invalid key `{key}`"
			));
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

/// Get language filter codes for home sections from default settings.
fn get_home_languages() -> Vec<String> {
	// Home sections use the default language options (JPN, ENG, NM etc.)
	// We don't filter by language in home sections — show all.
	Vec::new()
}

register_source!(DlsiteExplore, ListingProvider, Home, WebLoginHandler);

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
