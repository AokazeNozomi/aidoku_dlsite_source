#![allow(non_snake_case)]

use aidoku::{
	ContentRating, Manga,
	alloc::{format, vec, String, Vec},
	serde::Deserialize,
};

// ---------------------------------------------------------------------------
// Public product JSON API response (/api/=/product.json)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct PublicWorkGenre {
	#[serde(default)]
	pub name: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct PublicWorkImage {
	#[serde(default)]
	pub url: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct PublicWorkCreators {
	#[serde(default)]
	pub created_by: Option<Vec<PublicWorkCreator>>,
	#[serde(default)]
	pub scenario_by: Option<Vec<PublicWorkCreator>>,
	#[serde(default)]
	pub illust_by: Option<Vec<PublicWorkCreator>>,
	#[serde(default)]
	#[allow(dead_code)]
	pub voice_by: Option<Vec<PublicWorkCreator>>,
}

#[derive(Deserialize, Clone)]
pub struct PublicWorkCreator {
	#[serde(default)]
	pub name: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct PublicWork {
	#[serde(default)]
	pub workno: Option<String>,
	#[serde(default)]
	pub work_name: Option<String>,
	#[serde(default)]
	pub work_type: Option<String>,
	#[serde(default)]
	pub age_category: Option<u8>,
	#[serde(default)]
	pub maker_name: Option<String>,
	#[serde(default)]
	pub image_main: Option<PublicWorkImage>,
	#[serde(default)]
	pub intro_s: Option<String>,
	#[serde(default)]
	pub genres: Option<Vec<PublicWorkGenre>>,
	#[serde(default)]
	pub creaters: Option<PublicWorkCreators>,
	#[serde(default)]
	pub series_name: Option<String>,
	#[serde(default)]
	#[allow(dead_code)]
	pub regist_date: Option<String>,
}

impl PublicWork {
	fn cover_url(&self) -> Option<String> {
		let url = self.image_main.as_ref()?.url.as_ref()?;
		if url.starts_with("//") {
			Some(format!("https:{}", url))
		} else {
			Some(url.clone())
		}
	}

	pub fn into_manga(self) -> Manga {
		let workno = self.workno.clone().unwrap_or_default();
		let title = self.work_name.clone().unwrap_or_else(|| workno.clone());

		let content_rating = match self.age_category {
			Some(3) => ContentRating::NSFW,
			Some(2) => ContentRating::Suggestive,
			_ => ContentRating::Safe,
		};

		// Build description
		let mut desc_lines: Vec<String> = Vec::new();
		if let Some(ref maker) = self.maker_name {
			desc_lines.push(format!("Circle: {}", maker));
		}
		if let Some(ref series) = self.series_name {
			if !series.is_empty() {
				desc_lines.push(format!("Series: {}", series));
			}
		}
		if let Some(ref intro) = self.intro_s {
			if !intro.is_empty() {
				desc_lines.push(String::new());
				desc_lines.push(intro.clone());
			}
		}
		let description = if desc_lines.is_empty() {
			None
		} else {
			Some(desc_lines.join("\n"))
		};

		// Authors from creators
		let mut authors: Vec<String> = Vec::new();
		if let Some(ref creators) = self.creaters {
			if let Some(ref list) = creators.created_by {
				for c in list {
					if let Some(ref name) = c.name {
						if !authors.contains(name) {
							authors.push(name.clone());
						}
					}
				}
			}
			if let Some(ref list) = creators.scenario_by {
				for c in list {
					if let Some(ref name) = c.name {
						if !authors.contains(name) {
							authors.push(name.clone());
						}
					}
				}
			}
		}
		let authors = if authors.is_empty() {
			self.maker_name.as_ref().map(|m| vec![m.clone()])
		} else {
			Some(authors)
		};

		// Artists from illustrators
		let mut artists: Vec<String> = Vec::new();
		if let Some(ref creators) = self.creaters {
			if let Some(ref list) = creators.illust_by {
				for c in list {
					if let Some(ref name) = c.name {
						if !artists.contains(name) {
							artists.push(name.clone());
						}
					}
				}
			}
		}
		let artists = if artists.is_empty() { None } else { Some(artists) };

		// Tags: work type + genres
		let mut tags: Vec<String> = Vec::new();
		let type_label = match self.work_type.as_deref() {
			Some("MNG") => Some("Manga"),
			Some("WBT") => Some("Webtoon"),
			Some("CG") | Some("ICG") => Some("CG / Illustration"),
			Some("SOU") => Some("Sound / Voice"),
			Some("MOV") => Some("Video"),
			Some("NOV") | Some("NRE") => Some("Novel"),
			Some("GAM") | Some("ACN") | Some("RPG") | Some("ADV") | Some("SLN") => Some("Game"),
			Some("ETC") => Some("Other"),
			_ => None,
		};
		if let Some(label) = type_label {
			tags.push(label.into());
		}
		if let Some(ref genres) = self.genres {
			for g in genres {
				if let Some(ref name) = g.name {
					if !tags.contains(name) {
						tags.push(name.clone());
					}
				}
			}
		}
		let tags = if tags.is_empty() { None } else { Some(tags) };

		let url = Some(format!(
			"https://www.dlsite.com/maniax/work/=/product_id/{}.html",
			workno
		));

		Manga {
			key: workno,
			title,
			cover: self.cover_url(),
			authors,
			artists,
			description,
			tags,
			content_rating,
			url,
			..Default::default()
		}
	}
}
