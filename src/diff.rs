use crate::{FontDiff, FontInfo};
use read_fonts::types::GlyphId;
use std::collections::{HashMap, HashSet};

pub fn diff_fonts(font_a: &FontInfo, font_b: &FontInfo) -> FontDiff {
    let mut result = FontDiff::default();
    let mut processed_a = HashSet::with_capacity(font_a.glyph_ids.len());
    let mut processed_b = HashSet::with_capacity(font_b.glyph_ids.len());

    let mut common_unicodes: Vec<_> = font_a
        .unicode_to_id
        .keys()
        .filter(|unicode| font_b.unicode_to_id.contains_key(unicode))
        .copied()
        .collect();
    common_unicodes.sort_unstable();

    for unicode in common_unicodes {
        let gid_a = font_a.unicode_to_id[&unicode];
        let gid_b = font_b.unicode_to_id[&unicode];
        processed_a.insert(gid_a);
        processed_b.insert(gid_b);

        let name_a = glyph_name(font_a, gid_a);
        let name_b = glyph_name(font_b, gid_b);
        let same_shape = font_a.hashes.get(&gid_a) == font_b.hashes.get(&gid_b);
        match (name_a == name_b, same_shape) {
            (true, false) => result.changed.push(name_a),
            (false, true) => result.renamed.push((name_a, name_b)),
            (false, false) => result.modified.push((name_a, name_b)),
            (true, true) => {}
        }
    }

    let mut b_by_hash = HashMap::new();
    for &gid in font_b
        .glyph_ids
        .iter()
        .filter(|gid| !processed_b.contains(gid))
    {
        if let Some(&hash) = font_b.hashes.get(&gid) {
            b_by_hash.entry(hash).or_insert_with(Vec::new).push(gid);
        }
    }

    for &gid_a in font_a
        .glyph_ids
        .iter()
        .filter(|gid| !processed_a.contains(gid))
    {
        let matched = font_a
            .hashes
            .get(&gid_a)
            .and_then(|hash| b_by_hash.get_mut(hash))
            .and_then(Vec::pop);
        if let Some(gid_b) = matched {
            result
                .renamed
                .push((glyph_name(font_a, gid_a), glyph_name(font_b, gid_b)));
            processed_b.insert(gid_b);
        } else {
            result.removed.push(glyph_name(font_a, gid_a));
        }
    }

    result.added.extend(
        font_b
            .glyph_ids
            .iter()
            .filter(|gid| !processed_b.contains(gid))
            .map(|&gid| glyph_name(font_b, gid)),
    );

    result.added.sort_unstable();
    result.removed.sort_unstable();
    result.changed.sort_unstable();
    result.renamed.sort_unstable();
    result.modified.sort_unstable();
    result
}

fn glyph_name(font: &FontInfo, gid: GlyphId) -> String {
    font.id_to_name
        .get(&gid)
        .cloned()
        .unwrap_or_else(|| format!("gid{}", gid.to_u32()))
}
