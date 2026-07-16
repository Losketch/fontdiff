use crate::hash::{compute_hashes, GlyphHash};
use read_fonts::{tables::post::DEFAULT_GLYPH_NAMES, types::GlyphId, TableProvider};
use skrifa::{charmap::Charmap, font::FontRef, GlyphNames};
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
macro_rules! log_time {
    ($($arg:tt)*) => { web_sys::console::log_1(&format!($($arg)*).into()) };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! log_time {
    ($($arg:tt)*) => { eprintln!($($arg)*) };
}

pub struct FontInfo {
    pub glyph_ids: Vec<GlyphId>,
    pub name_to_id: HashMap<String, GlyphId>,
    pub id_to_name: HashMap<GlyphId, String>,
    pub unicode_to_id: HashMap<char, GlyphId>,
    pub id_to_unicode: HashMap<GlyphId, char>,
    pub(crate) hashes: HashMap<GlyphId, GlyphHash>,
}

pub fn init_font_info(font: FontRef<'_>) -> FontInfo {
    let num_glyphs = font
        .maxp()
        .map(|table| table.num_glyphs() as usize)
        .unwrap_or(0);
    let glyph_ids: Vec<_> = (0..num_glyphs)
        .map(|index| GlyphId::new(index as u32))
        .collect();
    log_time!("    [init] Parse metadata: {num_glyphs} glyphs");

    let (mut name_to_id, mut id_to_name) = read_post_names(&font, num_glyphs);
    if id_to_name.len() < 100 {
        let glyph_names = GlyphNames::new(&font);
        for &gid in &glyph_ids {
            if let Some(name) = glyph_names.get(gid) {
                let name = name.as_str().to_owned();
                name_to_id.insert(name.clone(), gid);
                id_to_name.insert(gid, name);
            }
        }
    }
    log_time!("    [init] Glyph names: {} named", id_to_name.len());

    let mut unicode_to_id = HashMap::new();
    let mut id_to_unicode = HashMap::new();
    for (codepoint, gid) in Charmap::new(&font).mappings() {
        if let Some(ch) = char::from_u32(codepoint) {
            unicode_to_id.insert(ch, gid);
            id_to_unicode.insert(gid, ch);
        }
    }
    log_time!(
        "    [init] Unicode mappings: {} mappings",
        unicode_to_id.len()
    );

    let hashes = compute_hashes(&font, &glyph_ids);
    log_time!("    [init] Compute hashes: {} hashes", hashes.len());

    FontInfo {
        glyph_ids,
        name_to_id,
        id_to_name,
        unicode_to_id,
        id_to_unicode,
        hashes,
    }
}

fn read_post_names(
    font: &FontRef<'_>,
    num_glyphs: usize,
) -> (HashMap<String, GlyphId>, HashMap<GlyphId, String>) {
    let Ok(post) = font.post() else {
        return (HashMap::new(), HashMap::new());
    };
    // `post` format 3 deliberately contains no glyph names. It also has no
    // glyph_name_index, just like format 1, so checking the optional array
    // alone would misclassify it and invent 258 Macintosh names for a CFF OTF.
    // Returning an empty map lets GlyphNames fall back to the CFF charset.
    if post.num_names() == 0 {
        return (HashMap::new(), HashMap::new());
    }
    let mut name_to_id = HashMap::new();
    let mut id_to_name = HashMap::new();

    if let Some(indices) = post.glyph_name_index() {
        // `post` stores custom names as variable-length Pascal strings. Calling
        // glyph_name for every id repeatedly scans from the start (O(n²));
        // collecting this array once makes the whole operation linear.
        let custom_names: Vec<_> = post
            .string_data()
            .into_iter()
            .flat_map(|strings| strings.iter())
            .filter_map(Result::ok)
            .map(|name| name.as_str())
            .collect();
        for (index, name_index) in indices.iter().take(num_glyphs).enumerate() {
            let name_index = name_index.get() as usize;
            let name = if name_index < DEFAULT_GLYPH_NAMES.len() {
                Some(DEFAULT_GLYPH_NAMES[name_index])
            } else {
                custom_names
                    .get(name_index - DEFAULT_GLYPH_NAMES.len())
                    .copied()
            };
            if let Some(name) = name {
                insert_name(&mut name_to_id, &mut id_to_name, index, name);
            }
        }
    } else {
        // Format 1 has only the standard 258 Macintosh names.
        for (index, &name) in DEFAULT_GLYPH_NAMES.iter().take(num_glyphs).enumerate() {
            insert_name(&mut name_to_id, &mut id_to_name, index, name);
        }
    }
    (name_to_id, id_to_name)
}

fn insert_name(
    name_to_id: &mut HashMap<String, GlyphId>,
    id_to_name: &mut HashMap<GlyphId, String>,
    index: usize,
    name: &str,
) {
    let gid = GlyphId::new(index as u32);
    let name = name.to_owned();
    name_to_id.insert(name.clone(), gid);
    id_to_name.insert(gid, name);
}

pub fn glyph_to_char(font: &FontInfo, glyph_name: &str) -> Option<char> {
    font.name_to_id
        .get(glyph_name)
        .and_then(|gid| font.id_to_unicode.get(gid))
        .copied()
}
