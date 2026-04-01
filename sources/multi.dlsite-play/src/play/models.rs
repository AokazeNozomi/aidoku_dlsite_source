use aidoku::{
	alloc::{String, Vec, collections::BTreeMap, format},
	serde::Deserialize,
};

// ---------------------------------------------------------------------------
// Download token from GET /api/v3/download/sign/cookie
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct DownloadToken {
	#[allow(dead_code)]
	pub expires: String,
	pub url: String,
}

// ---------------------------------------------------------------------------
// PlayFile from ziptree.json playfile entries
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct OptimizedInfo {
	pub name: Option<String>,
	#[allow(dead_code)]
	pub length: Option<i64>,
	pub width: Option<i32>,
	pub height: Option<i32>,
	pub crypt: Option<bool>,
}

#[derive(Deserialize, Clone)]
pub struct PdfPageOptimized {
	pub name: Option<String>,
	pub length: Option<i64>,
	pub width: Option<i32>,
	pub height: Option<i32>,
	pub crypt: Option<bool>,
}

#[derive(Deserialize, Clone)]
pub struct PdfPage {
	pub optimized: Option<PdfPageOptimized>,
}

#[derive(Deserialize, Clone)]
pub struct PlayFileFiles {
	pub optimized: Option<OptimizedInfo>,
	pub page: Option<Vec<PdfPage>>,
}

#[derive(Clone)]
pub struct PlayFile {
	#[allow(dead_code)]
	pub length: i64,
	pub file_type: String,
	pub files: PlayFileFiles,
	#[allow(dead_code)]
	pub hashname: String,
}

impl PlayFile {
	pub fn optimized_name(&self) -> Option<&str> {
		self.files
			.optimized
			.as_ref()
			.and_then(|o| o.name.as_deref())
	}

	pub fn is_crypt(&self) -> bool {
		self.files
			.optimized
			.as_ref()
			.and_then(|o| o.crypt)
			.unwrap_or(false)
	}

	pub fn crypt_dimensions(&self) -> Option<(i32, i32)> {
		let opt = self.files.optimized.as_ref()?;
		Some((opt.width?, opt.height?))
	}
}

/// Raw JSON shape for a playfile entry inside ziptree.json.
/// The `type` field is a reserved keyword, so we deserialize into this
/// intermediate struct and then convert to `PlayFile`.
#[derive(Deserialize)]
pub struct RawPlayFile {
	pub length: i64,
	#[serde(rename = "type")]
	pub file_type: String,
	#[serde(default)]
	pub image: Option<PlayFileFiles>,
	#[serde(default)]
	pub pdf: Option<PlayFileFiles>,
	#[serde(default)]
	pub video: Option<PlayFileFiles>,
	#[serde(default)]
	pub ebook_fixed: Option<PlayFileFiles>,
	#[serde(default)]
	pub epub: Option<PlayFileFiles>,
	#[serde(default)]
	pub epub_reflowable: Option<PlayFileFiles>,
	#[serde(default)]
	pub voicecomic_v2: Option<PlayFileFiles>,
}

impl RawPlayFile {
	pub fn into_playfile(self, hashname: String) -> PlayFile {
		let files = match self.file_type.as_str() {
			"image" => self.image,
			"pdf" => self.pdf,
			"video" => self.video,
			"ebook_fixed" => self.ebook_fixed,
			"epub" => self.epub,
			"epub_reflowable" => self.epub_reflowable,
			"voicecomic_v2" => self.voicecomic_v2,
			_ => None,
		};
		PlayFile {
			length: self.length,
			file_type: self.file_type,
			files: files.unwrap_or(PlayFileFiles {
				optimized: None,
				page: None,
			}),
			hashname,
		}
	}
}

// ---------------------------------------------------------------------------
// ZipTree from GET {token.url}ziptree.json
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RawTreeEntry {
	#[serde(rename = "type")]
	pub entry_type: String,
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub path: Option<String>,
	#[serde(default)]
	pub hashname: Option<String>,
	#[serde(default)]
	pub children: Option<Vec<RawTreeEntry>>,
}

#[derive(Deserialize)]
pub struct RawZipTree {
	pub hash: String,
	#[serde(default)]
	pub playfile: BTreeMap<String, RawPlayFile>,
	#[serde(default)]
	pub tree: Vec<RawTreeEntry>,
}

pub struct ZipTree {
	#[allow(dead_code)]
	pub hash: String,
	pub playfiles: BTreeMap<String, PlayFile>,
	pub tree: Vec<RawTreeEntry>,
}

impl ZipTree {
	pub fn from_raw(raw: RawZipTree) -> Self {
		let playfiles: BTreeMap<String, PlayFile> = raw
			.playfile
			.into_iter()
			.map(|(k, v): (String, RawPlayFile)| {
				let pf = v.into_playfile(k.clone());
				(k, pf)
			})
			.collect();
		ZipTree {
			hash: raw.hash,
			playfiles,
			tree: raw.tree,
		}
	}

	/// Walk the tree and return `(relative_path, PlayFile)` pairs.
	pub fn walk(&self) -> Vec<(String, PlayFile)> {
		let mut result = Vec::new();
		walk_entries(&self.tree, None, &self.playfiles, &mut result);
		result
	}
}

fn walk_entries(
	entries: &[RawTreeEntry],
	parent: Option<&str>,
	playfiles: &BTreeMap<String, PlayFile>,
	out: &mut Vec<(String, PlayFile)>,
) {
	for entry in entries {
		match entry.entry_type.as_str() {
			"file" => {
				if let (Some(name), Some(hashname)) = (&entry.name, &entry.hashname) {
					let path: String = match parent {
						Some(p) => format!("{}/{}", p, name),
						None => name.clone(),
					};
					if let Some(pf) = playfiles.get(hashname.as_str()) {
						out.push((path, (*pf).clone()));
					}
				}
			}
			"folder" => {
				let folder_path = entry.path.as_deref().or(entry.name.as_deref());
				if let Some(children) = &entry.children {
					walk_entries(children, folder_path, playfiles, out);
				}
			}
			_ => {}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

	fn make_playfile(opt_name: Option<&str>, crypt: Option<bool>, w: Option<i32>, h: Option<i32>) -> PlayFile {
		PlayFile {
			length: 100,
			file_type: "image".into(),
			files: PlayFileFiles {
				optimized: Some(OptimizedInfo {
					name: opt_name.map(|s| s.into()),
					length: Some(50),
					width: w,
					height: h,
					crypt,
				}),
				page: None,
			},
			hashname: "hash".into(),
		}
	}

	#[aidoku_test]
	fn playfile_optimized_name_present() {
		let pf = make_playfile(Some("opt.webp"), None, None, None);
		assert_eq!(pf.optimized_name(), Some("opt.webp"));
	}

	#[aidoku_test]
	fn playfile_optimized_name_missing() {
		let pf = PlayFile {
			length: 100,
			file_type: "image".into(),
			files: PlayFileFiles {
				optimized: None,
				page: None,
			},
			hashname: "hash".into(),
		};
		assert_eq!(pf.optimized_name(), None);
	}

	#[aidoku_test]
	fn playfile_is_crypt_true() {
		let pf = make_playfile(Some("x.webp"), Some(true), None, None);
		assert!(pf.is_crypt());
	}

	#[aidoku_test]
	fn playfile_is_crypt_false() {
		let pf = make_playfile(Some("x.webp"), Some(false), None, None);
		assert!(!pf.is_crypt());
	}

	#[aidoku_test]
	fn playfile_is_crypt_none() {
		let pf = make_playfile(Some("x.webp"), None, None, None);
		assert!(!pf.is_crypt());
	}

	#[aidoku_test]
	fn playfile_crypt_dimensions() {
		let pf = make_playfile(Some("x.webp"), Some(true), Some(800), Some(600));
		assert_eq!(pf.crypt_dimensions(), Some((800, 600)));
	}

	#[aidoku_test]
	fn playfile_crypt_dimensions_missing() {
		let pf = make_playfile(Some("x.webp"), Some(true), None, None);
		assert_eq!(pf.crypt_dimensions(), None);
	}
}
