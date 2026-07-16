use read_fonts::{tables::hmtx::Hmtx, types::GlyphId};
use skrifa::{
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlineGlyphCollection, OutlinePen},
    MetadataProvider,
};
use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

/// A compact, deterministic structural fingerprint. This is intentionally not
/// cryptographic: collision resistance beyond 128 bits does not help a local
/// font comparison, while SHA-256 is particularly expensive in single-threaded
/// WebAssembly.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GlyphHash(u64, u64);

struct GlyphHasher {
    a: u64,
    b: u64,
    words: u64,
}

impl GlyphHasher {
    fn new() -> Self {
        Self {
            a: 0x243f_6a88_85a3_08d3,
            b: 0x1319_8a2e_0370_7344,
            words: 0,
        }
    }

    #[inline]
    fn write(&mut self, value: u32) {
        let value = value as u64;
        self.a ^= value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        self.a = self.a.rotate_left(27).wrapping_mul(0x3c79_ac49_2ba7_b653);
        self.b ^= self.a.wrapping_add(value.rotate_left(17));
        self.b = self.b.rotate_left(31).wrapping_mul(0x1c69_b3f7_4ac4_ae35);
        self.words += 1;
    }

    fn finish(mut self) -> GlyphHash {
        self.a ^= self.words.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.b ^= self.words.wrapping_mul(0xd6e8_feb8_6659_fd93);
        self.a ^= self.b.rotate_left(23);
        self.b ^= self.a.rotate_left(41);
        GlyphHash(avalanche(self.a), avalanche(self.b))
    }
}

#[inline]
fn avalanche(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

struct HasherPen<'a>(&'a mut GlyphHasher);

impl HasherPen<'_> {
    #[inline]
    fn point(&mut self, x: f32, y: f32) {
        self.0.write(x.to_bits());
        self.0.write(y.to_bits());
    }
}

impl OutlinePen for HasherPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.write(b'M' as u32);
        self.point(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.write(b'L' as u32);
        self.point(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.write(b'Q' as u32);
        self.point(cx0, cy0);
        self.point(x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.write(b'C' as u32);
        self.point(cx0, cy0);
        self.point(cx1, cy1);
        self.point(x, y);
    }

    fn close(&mut self) {
        self.0.write(b'Z' as u32);
    }
}

fn glyph_hash(
    outlines: &OutlineGlyphCollection<'_>,
    glyph_id: GlyphId,
    hmtx: Option<&Hmtx<'_>>,
) -> Option<GlyphHash> {
    let mut hasher = GlyphHasher::new();

    if let Some(glyph) = outlines.get(glyph_id) {
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
        glyph.draw(settings, &mut HasherPen(&mut hasher)).ok()?;
    }

    hasher.write(hmtx.and_then(|table| table.advance(glyph_id)).unwrap_or(0) as u32);
    Some(hasher.finish())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn compute_hashes(
    font: &skrifa::font::FontRef<'_>,
    glyph_ids: &[GlyphId],
) -> HashMap<GlyphId, GlyphHash> {
    use read_fonts::TableProvider;

    let hmtx = font.hmtx().ok();
    let outlines = font.outline_glyphs();
    glyph_ids
        .par_iter()
        .filter_map(|&gid| glyph_hash(&outlines, gid, hmtx.as_ref()).map(|hash| (gid, hash)))
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn compute_hashes(
    font: &skrifa::font::FontRef<'_>,
    glyph_ids: &[GlyphId],
) -> HashMap<GlyphId, GlyphHash> {
    use read_fonts::TableProvider;

    let hmtx = font.hmtx().ok();
    let outlines = font.outline_glyphs();
    glyph_ids
        .iter()
        .filter_map(|&gid| glyph_hash(&outlines, gid, hmtx.as_ref()).map(|hash| (gid, hash)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_deterministic_and_order_sensitive() {
        let mut first = GlyphHasher::new();
        first.write(1);
        first.write(2);
        let mut same = GlyphHasher::new();
        same.write(1);
        same.write(2);
        let mut reversed = GlyphHasher::new();
        reversed.write(2);
        reversed.write(1);

        let first = first.finish();
        let same = same.finish();
        assert_eq!(first, same);
        assert_ne!(same, reversed.finish());
    }
}
