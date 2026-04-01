use aidoku::{
	alloc::{String, Vec},
	imports::defaults::defaults_get,
};

// ---------------------------------------------------------------------------
// Content rating
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContentRatingFilter {
	All = 0,
	Safe = 1,
	R15 = 2,
	R18 = 3,
}

impl ContentRatingFilter {
	pub fn from_setting(s: Option<&str>) -> Self {
		match s {
			Some("Safe") => Self::Safe,
			Some("R-15") => Self::R15,
			Some("R-18") => Self::R18,
			_ => Self::All,
		}
	}

	/// Convert to a filter string for API calls.
	/// Returns `None` for `All` (no filtering).
	pub fn to_filter_string(self) -> Option<String> {
		match self {
			Self::Safe => Some("safe".into()),
			Self::R15 => Some("r15".into()),
			Self::R18 => Some("r18".into()),
			Self::All => None,
		}
	}
}

pub fn get_default_content_rating() -> ContentRatingFilter {
	ContentRatingFilter::from_setting(defaults_get::<String>("default_content_rating").as_deref())
}

// ---------------------------------------------------------------------------
// Work type setting
// ---------------------------------------------------------------------------

/// Read work type filter from settings multi-selects.
/// Returns the list of enabled work type codes.
pub fn get_work_type_setting() -> Vec<String> {
	let keys = [
		"wt_images", "wt_av", "wt_game", "wt_tools", "wt_misc",
	];
	let mut selected = Vec::new();
	for key in &keys {
		if let Some(values) = defaults_get::<Vec<String>>(key) {
			selected.extend(values);
		}
	}
	selected
}
