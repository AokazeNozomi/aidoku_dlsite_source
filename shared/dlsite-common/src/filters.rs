use crate::explore::ExploreSort;
use crate::settings::DlsiteLang;
use aidoku::{
	alloc::{String, Vec},
	FilterValue,
};

pub fn extract_language_filter(filters: &[FilterValue]) -> Vec<DlsiteLang> {
	for f in filters {
		if let FilterValue::MultiSelect { id, included, .. } = f {
			if id == "language" && !included.is_empty() {
				return included
					.iter()
					.filter_map(|s| DlsiteLang::from_api_code(s))
					.collect();
			}
		}
	}
	Vec::new()
}

pub fn extract_sort_filter(filters: &[FilterValue]) -> ExploreSort {
	for f in filters {
		if let FilterValue::Sort { index, .. } = f {
			return ExploreSort::from_index(*index);
		}
	}
	ExploreSort::Trending
}

pub fn extract_work_type_filter(filters: &[FilterValue]) -> Vec<String> {
	for f in filters {
		if let FilterValue::MultiSelect { id, included, .. } = f {
			if id == "work_type" && !included.is_empty() {
				return included.clone();
			}
		}
	}
	Vec::new()
}

pub fn extract_content_rating_filter(filters: &[FilterValue]) -> Vec<String> {
	for f in filters {
		if let FilterValue::MultiSelect { id, included, .. } = f {
			if id == "content_rating" && !included.is_empty() {
				return included.clone();
			}
		}
	}
	Vec::new()
}

pub fn extract_site_filter<'a>(filters: &[FilterValue], slugs: &[&'a str]) -> &'a str {
	for f in filters {
		if let FilterValue::Select { id, value, .. } = f {
			if id == "site" {
				if let Some(slug) = slugs.iter().find(|&&s| s == value.as_str()) {
					return slug;
				}
			}
		}
	}
	slugs[0]
}

pub fn extract_genre_filter(filters: &[FilterValue]) -> Vec<u32> {
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
