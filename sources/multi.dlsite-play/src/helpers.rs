use aidoku::{
	Result,
	alloc::{String, Vec, collections::BTreeMap, format, vec},
	canvas::Rect,
	imports::canvas::{Canvas, ImageRef},
	prelude::*,
};

use crate::models::{PlayFile, ZipTree};

// ---------------------------------------------------------------------------
// MT19937 PRNG (DLsite Play's init_genrand variant)
// ---------------------------------------------------------------------------

const MT_N: usize = 624;

struct Mt19937 {
	mt: [u32; MT_N],
	index: usize,
}

impl Mt19937 {
	fn new(seed: u32) -> Self {
		let mut mt = [seed; MT_N];
		for i in 1..MT_N {
			mt[i] = (1812433253u32)
				.wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
				.wrapping_add(i as u32);
			mt[i] &= 0xFFFF_FFFF;
		}
		Self { mt, index: MT_N }
	}

	fn generate_numbers(&mut self) {
		for i in 0..MT_N {
			let y = (self.mt[i] & 0x8000_0000) | (self.mt[(i + 1) % MT_N] & 0x7FFF_FFFF);
			self.mt[i] = self.mt[(i + 397) % MT_N] ^ (y >> 1);
			if y & 1 != 0 {
				self.mt[i] ^= 0x9908_B0DF;
			}
		}
	}

	fn next_u32(&mut self) -> u32 {
		if self.index >= MT_N {
			self.generate_numbers();
			self.index = 0;
		}
		let mut y = self.mt[self.index];
		y ^= y >> 11;
		y ^= (y << 7) & 0x9D2C_5680;
		y ^= (y << 15) & 0xEFC6_0000;
		y ^= y >> 18;
		self.index += 1;
		y
	}

	/// Random float in [0, 1) matching Python's random.random() behavior.
	fn random(&mut self) -> f64 {
		let a = (self.next_u32() >> 5) as f64;
		let b = (self.next_u32() >> 6) as f64;
		(a * 67108864.0 + b) / 9007199254740992.0
	}
}

/// Generate the tile permutation array used by DLsite Play's image scrambler.
///
/// Replicates the shuffle from dlsite-async's `_mt_tiles`:
/// iterates backwards, swapping each position with a random earlier one,
/// resetting the MT index counter after each step.
pub fn mt_tiles(seed: u32, length: usize) -> Vec<usize> {
	let mut rng = Mt19937::new(seed);
	let mut a: Vec<usize> = (0..length).collect();
	let mut pos = 0usize;

	for n in (0..length).rev() {
		let e = (rng.random() * (n + 1) as f64) as usize;
		a.swap(n, e);

		// DLsite's MT implementation resets the index counter after each step
		pos += 1;
		rng.index = pos;
	}

	a
}

// ---------------------------------------------------------------------------
// Image descrambling
// ---------------------------------------------------------------------------

/// Descramble a DLsite Play encrypted image.
///
/// The image is split into 128x128px tiles and shuffled using MT19937.
/// We reverse the shuffle by computing the permutation and placing
/// tiles back in correct order via Canvas copy operations.
pub fn descramble_image(
	image: &ImageRef,
	optimized_name: &str,
	width: i32,
	height: i32,
) -> Result<ImageRef> {
	const TILE_W: i32 = 128;

	let tiles_w = (width + TILE_W - 1) / TILE_W;
	let tiles_h = (height + TILE_W - 1) / TILE_W;
	let tile_count = (tiles_w * tiles_h) as usize;

	let seed_str = if optimized_name.len() >= 12 {
		&optimized_name[5..12]
	} else {
		bail!("optimized_name too short for seed extraction");
	};
	let seed = u32::from_str_radix(seed_str, 16)
		.map_err(|_| error!("Invalid hex seed in optimized_name: {}", seed_str))?;

	let tile_order = mt_tiles(seed, tile_count);

	// Build reverse mapping: shuffle[i] = source tile index for output position i
	let mut shuffle = vec![0usize; tile_count];
	for (v, &k) in tile_order.iter().enumerate() {
		if k < tile_count {
			shuffle[k] = v;
		}
	}

	let img_w = image.width();
	let img_h = image.height();
	let mut canvas = Canvas::new(img_w, img_h);

	for (i, &src_idx) in shuffle.iter().enumerate() {
		let src_x = (src_idx % tiles_w as usize) as f32 * TILE_W as f32;
		let src_y = (src_idx / tiles_w as usize) as f32 * TILE_W as f32;

		let dst_x = (i % tiles_w as usize) as f32 * TILE_W as f32;
		let dst_y = (i / tiles_w as usize) as f32 * TILE_W as f32;

		let src_rect = Rect::new(src_x, src_y, TILE_W as f32, TILE_W as f32);
		let dst_rect = Rect::new(dst_x, dst_y, TILE_W as f32, TILE_W as f32);

		canvas.copy_image(image, src_rect, dst_rect);
	}

	// Crop to actual dimensions if needed
	if width as f32 != img_w || height as f32 != img_h {
		let mut cropped = Canvas::new(width as f32, height as f32);
		cropped.copy_image(
			&canvas.get_image(),
			Rect::new(0.0, 0.0, width as f32, height as f32),
			Rect::new(0.0, 0.0, width as f32, height as f32),
		);
		return Ok(cropped.get_image());
	}

	Ok(canvas.get_image())
}

// ---------------------------------------------------------------------------
// Page extraction from ziptree
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ChapterGroup {
	pub key: String,
	pub title: String,
	pub pages: Vec<(String, PlayFile)>,
}

/// Extract ordered chapter groups from a ziptree.
///
/// - Images are grouped by their full parent folder path (any depth).
/// - PDF files are expanded and each PDF file path becomes its own chapter.
pub fn extract_chapter_groups(tree: &ZipTree) -> Vec<ChapterGroup> {
	let entries = tree.walk();

	let mut image_groups: BTreeMap<String, Vec<(String, PlayFile)>> = BTreeMap::new();
	let mut pdf_groups: BTreeMap<String, Vec<(String, PlayFile)>> = BTreeMap::new();

	for (path, pf) in entries {
		match pf.file_type.as_str() {
			"image" => {
				if pf.optimized_name().is_some() {
					let folder = parent_folder_path(&path);
					image_groups.entry(folder).or_default().push((path, pf));
				}
			}
			"pdf" => {
				let pages = expand_pdf_pages(&path, &pf);
				if !pages.is_empty() {
					pdf_groups.entry(path).or_default().extend(pages);
				}
			}
			_ => {}
		}
	}

	let mut chapters: Vec<ChapterGroup> = Vec::new();

	for (folder, mut pages) in image_groups {
		pages.sort_by(|a, b| natural_cmp(&a.0, &b.0));
		let title = if folder == "root" {
			"root".into()
		} else {
			folder.clone()
		};
		chapters.push(ChapterGroup {
			key: format!("img:{}", folder),
			title,
			pages,
		});
	}

	for (pdf_path, mut pages) in pdf_groups {
		pages.sort_by(|a, b| natural_cmp(&a.0, &b.0));
		chapters.push(ChapterGroup {
			key: format!("pdf:{}", pdf_path),
			title: pdf_path,
			pages,
		});
	}

	chapters.sort_by(|a, b| natural_cmp(&a.title, &b.title));
	chapters
}

/// Expand a PDF PlayFile into individual page PlayFiles.
fn expand_pdf_pages(path: &str, playfile: &PlayFile) -> Vec<(String, PlayFile)> {
	let pages = match &playfile.files.page {
		Some(pages) => pages,
		None => return Vec::new(),
	};

	let mut result = Vec::new();
	for (idx, page) in pages.iter().enumerate() {
		let opt = match &page.optimized {
			Some(o) if o.name.is_some() => o,
			_ => continue,
		};

		let synthetic = PlayFile {
			length: opt.length.unwrap_or(0),
			file_type: "image".into(),
			files: crate::models::PlayFileFiles {
				optimized: Some(crate::models::OptimizedInfo {
					name: opt.name.clone(),
					length: opt.length,
					width: opt.width,
					height: opt.height,
					crypt: opt.crypt,
				}),
				page: None,
			},
			hashname: opt.name.clone().unwrap_or_default(),
		};

		let page_path = format!("{}#{:04}", path, idx);
		result.push((page_path, synthetic));
	}

	result
}

fn parent_folder_path(path: &str) -> String {
	match path.rsplit_once('/') {
		Some((parent, _)) if !parent.is_empty() => parent.into(),
		_ => "root".into(),
	}
}

// ---------------------------------------------------------------------------
// Natural sort comparison
// ---------------------------------------------------------------------------

enum NatChunk<'a> {
	Text(&'a str),
	Num(u64),
}

fn natural_chunks(s: &str) -> Vec<NatChunk<'_>> {
	let mut chunks = Vec::new();
	let mut i = 0;
	let bytes = s.as_bytes();
	while i < bytes.len() {
		if bytes[i].is_ascii_digit() {
			let start = i;
			while i < bytes.len() && bytes[i].is_ascii_digit() {
				i += 1;
			}
			let num_str = &s[start..i];
			let num = num_str.parse::<u64>().unwrap_or(0);
			chunks.push(NatChunk::Num(num));
		} else {
			let start = i;
			while i < bytes.len() && !bytes[i].is_ascii_digit() {
				i += 1;
			}
			chunks.push(NatChunk::Text(&s[start..i]));
		}
	}
	chunks
}

fn natural_cmp(a: &str, b: &str) -> core::cmp::Ordering {
	let ca = natural_chunks(a);
	let cb = natural_chunks(b);

	for (ac, bc) in ca.iter().zip(cb.iter()) {
		let ord = match (ac, bc) {
			(NatChunk::Num(na), NatChunk::Num(nb)) => na.cmp(nb),
			(NatChunk::Text(ta), NatChunk::Text(tb)) => ta.to_lowercase().cmp(&tb.to_lowercase()),
			(NatChunk::Num(_), NatChunk::Text(_)) => core::cmp::Ordering::Less,
			(NatChunk::Text(_), NatChunk::Num(_)) => core::cmp::Ordering::Greater,
		};
		if ord != core::cmp::Ordering::Equal {
			return ord;
		}
	}

	ca.len().cmp(&cb.len())
}
