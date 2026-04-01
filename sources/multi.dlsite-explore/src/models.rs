use aidoku::{
	alloc::{format, String, Vec},
	ContentRating, Manga,
};

pub use dlsite_common::models::PublicWork;

// ---------------------------------------------------------------------------
// Explore sort options (server-side via /fsr/ajax/=/)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExploreSort {
	Newest = 0,
	Trending = 1,
	Downloads = 2,
	Rating = 3,
}

impl ExploreSort {
	pub fn from_index(index: i32) -> Self {
		match index {
			1 => Self::Trending,
			2 => Self::Downloads,
			3 => Self::Rating,
			_ => Self::Newest,
		}
	}

	/// DLsite `order` path segment value.
	pub fn order_param(self) -> &'static str {
		match self {
			Self::Newest => "release_d",
			Self::Trending => "trend",
			Self::Downloads => "dl_d",
			Self::Rating => "rate_d",
		}
	}
}

// ---------------------------------------------------------------------------
// Search result models (parsed from /fsr/ajax/=/ HTML)
// ---------------------------------------------------------------------------

pub struct ExploreWork {
	pub workno: String,
	pub title: String,
	pub cover_url: Option<String>,
	pub maker_name: Option<String>,
	pub work_type: Option<String>,
	/// Raw age string from `__product_attributes`: `"adl"`, `"r15"`, or absent.
	pub age_category: Option<String>,
}

pub struct ExploreResult {
	pub works: Vec<ExploreWork>,
	pub has_next_page: bool,
}

impl ExploreWork {
	fn work_type_label(&self) -> Option<&'static str> {
		match self.work_type.as_deref()? {
			"MNG" => Some("Manga"),
			"SCM" => Some("Gekiga"),
			"WBT" => Some("Webtoon"),
			"ICG" => Some("CG / Illustration"),
			"NRE" => Some("Novel"),
			"DNV" => Some("Digital Novel"),
			"MOV" => Some("Video"),
			"SOU" => Some("Sound / Voice"),
			"MUS" => Some("Music"),
			"ACN" | "QIZ" | "ADV" | "RPG" | "TBL" | "SLN" | "TYP" | "STG" | "PZL" => {
				Some("Game")
			}
			"ETC" | "ET3" => Some("Other"),
			"TOL" => Some("Tools / Accessories"),
			"IMT" => Some("Illustration Materials"),
			"AMT" => Some("Music Materials"),
			"VCM" => Some("Voiced Comic"),
			"PBC" => Some("Publication"),
			_ => None,
		}
	}

	pub fn into_manga(self) -> Manga {
		let content_rating = match self.age_category.as_deref() {
			Some("adl") => ContentRating::NSFW,
			Some("r15") => ContentRating::Suggestive,
			_ => ContentRating::Safe,
		};

		let mut tags: Vec<String> = Vec::new();
		if let Some(label) = self.work_type_label() {
			tags.push(label.into());
		}

		let description = self.maker_name.as_ref().map(|m| format!("Circle: {}", m));

		let url = Some(format!(
			"https://www.dlsite.com/maniax/work/=/product_id/{}.html",
			self.workno
		));

		Manga {
			key: self.workno,
			title: self.title,
			cover: self.cover_url,
			description,
			tags: if tags.is_empty() { None } else { Some(tags) },
			content_rating,
			url,
			..Default::default()
		}
	}
}
