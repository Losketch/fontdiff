use std::collections::{HashMap, HashSet};
use base64::{engine::general_purpose, Engine as _};
use read_fonts::{types::GlyphId, TableProvider, tables::hmtx::Hmtx};
use sha2::{Digest, Sha256};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;
use skrifa::{
    charmap::Charmap,
    font::FontRef,
    GlyphNames,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
    MetadataProvider,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
macro_rules! js_console_log {
    ($($arg:tt)*) => {
        web_sys::console::log_1(&format!($($arg)*).into());
    };
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! js_console_log {
    ($($arg:tt)*) => {};
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! log_time {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
        js_console_log!($($arg)*);
    };
}

#[cfg(target_arch = "wasm32")]
macro_rules! log_time {
    ($($arg:tt)*) => {
        js_console_log!($($arg)*);
    };
}

struct HasherPen<'a>(&'a mut Sha256);

impl OutlinePen for HasherPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.update(b"M");
        self.0.update(&x.to_bits().to_le_bytes());
        self.0.update(&y.to_bits().to_le_bytes());
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.update(b"L");
        self.0.update(&x.to_bits().to_le_bytes());
        self.0.update(&y.to_bits().to_le_bytes());
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.update(b"Q");
        self.0.update(&cx0.to_bits().to_le_bytes());
        self.0.update(&cy0.to_bits().to_le_bytes());
        self.0.update(&x.to_bits().to_le_bytes());
        self.0.update(&y.to_bits().to_le_bytes());
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.update(b"C");
        self.0.update(&cx0.to_bits().to_le_bytes());
        self.0.update(&cy0.to_bits().to_le_bytes());
        self.0.update(&cx1.to_bits().to_le_bytes());
        self.0.update(&cy1.to_bits().to_le_bytes());
        self.0.update(&x.to_bits().to_le_bytes());
        self.0.update(&y.to_bits().to_le_bytes());
    }
    fn close(&mut self) {
        self.0.update(b"Z");
    }
}

fn glyph_hash_with_hmtx(font: &FontRef, glyph_id: GlyphId, hmtx: Option<&Hmtx>) -> Option<String> {
    let mut hasher = Sha256::new();
    let outlines = font.outline_glyphs();

    if let Some(glyph) = outlines.get(glyph_id) {
        let mut pen = HasherPen(&mut hasher);
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
        if glyph.draw(settings, &mut pen).is_err() {
            return None;
        }
    }

    let width: u16 = hmtx.and_then(|h| h.advance(glyph_id)).unwrap_or(0);
    hasher.update(&width.to_be_bytes());

    Some(format!("{:x}", hasher.finalize()))
}

#[cfg(not(target_arch = "wasm32"))]
fn compute_hashes(font: &FontRef, glyph_ids: &[GlyphId]) -> HashMap<GlyphId, String> {
    let hmtx = font.hmtx().ok();
    let hmtx_ref = hmtx.as_ref();
    let font_ref = font;
    let hashes: Vec<(GlyphId, String)> = glyph_ids
        .par_iter()
        .filter_map(|&gid| {
            glyph_hash_with_hmtx(font_ref, gid, hmtx_ref)
                .map(|hash| (gid, hash))
        })
        .collect();
    HashMap::from_iter(hashes)
}

#[cfg(target_arch = "wasm32")]
fn compute_hashes(font: &FontRef, glyph_ids: &[GlyphId]) -> HashMap<GlyphId, String> {
    let hmtx = font.hmtx().ok();
    let hmtx_ref = hmtx.as_ref();
    let mut hashes = HashMap::new();
    for &gid in glyph_ids {
        if let Some(hash) = glyph_hash_with_hmtx(font, gid, hmtx_ref) {
            hashes.insert(gid, hash);
        }
    }
    hashes
}

#[derive(Debug, Default)]
pub struct FontDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub renamed: Vec<(String, String)>,
    pub modified: Vec<(String, String)>,
}

pub struct FontInfo<'a> {
    font: FontRef<'a>,
    charmap: Charmap<'a>,
    pub glyph_ids: Vec<GlyphId>,
    pub name_to_id: HashMap<String, GlyphId>,
    pub id_to_name: HashMap<GlyphId, String>,
    pub unicode_to_id: HashMap<char, GlyphId>,
    pub id_to_unicode: HashMap<GlyphId, char>,
    pub hashes: HashMap<GlyphId, String>,
}

pub fn init_font_info(font: FontRef) -> FontInfo {
    let charmap = Charmap::new(&font);
    let num_glyphs = font
        .maxp()
        .map(|m| m.num_glyphs() as usize)
        .unwrap_or(0);

    let glyph_ids: Vec<GlyphId> = (0..num_glyphs).map(|i| GlyphId::new(i as u32)).collect();
    log_time!("    [init] Parse metadata: {} glyphs", num_glyphs);

    let mut name_to_id = HashMap::new();
    let mut id_to_name = HashMap::new();

    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(post) = font.post() {
            if post.glyph_name(0.into()).is_some() {
                let post_ref = &post;
                let names: Vec<(u32, String)> = (0u32..num_glyphs as u32)
                    .into_par_iter()
                    .filter_map(|i| {
                        post_ref.glyph_name((i as u16).into())
                            .map(|name| (i, name.to_string()))
                    })
                    .collect();
                for (i, name_str) in names {
                    let gid = GlyphId::new(i);
                    name_to_id.insert(name_str.clone(), gid);
                    id_to_name.insert(gid, name_str);
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        if let Ok(post) = font.post() {
            if post.glyph_name(0.into()).is_some() {
                for i in 0u32..num_glyphs as u32 {
                    if let Some(name) = post.glyph_name((i as u16).into()) {
                        let gid = GlyphId::new(i);
                        let name_str = name.to_string();
                        name_to_id.insert(name_str.clone(), gid);
                        id_to_name.insert(gid, name_str);
                    }
                }
            }
        }
    }

    if id_to_name.len() < 100 {
        let glyph_names = GlyphNames::new(&font);
        for &gid in &glyph_ids {
            if let Some(name) = glyph_names.get(gid) {
                let name_str = name.as_str().to_string();
                name_to_id.insert(name_str.clone(), gid);
                id_to_name.insert(gid, name_str);
            }
        }
    }

    log_time!("    [init] Glyph names: {} named", id_to_name.len());

    let mut unicode_to_id = HashMap::new();
    let mut id_to_unicode = HashMap::new();

    for mapping in charmap.mappings() {
        let (codepoint, gid) = mapping;
        if let Some(ch) = std::char::from_u32(codepoint) {
            unicode_to_id.insert(ch, gid);
            id_to_unicode.insert(gid, ch);
        }
    }
    log_time!("    [init] Unicode mappings: {} mappings", unicode_to_id.len());

    let hashes = compute_hashes(&font, &glyph_ids);
    log_time!("    [init] Compute hashes: {} hashes", hashes.len());
    log_time!("    [init] Total done");

    FontInfo {
        font,
        charmap,
        glyph_ids,
        name_to_id,
        id_to_name,
        unicode_to_id,
        id_to_unicode,
        hashes,
    }
}

pub fn diff_fonts(font_a: &FontInfo, font_b: &FontInfo) -> FontDiff {
    let mut result = FontDiff::default();
    let mut processed_a = HashSet::new();
    let mut processed_b = HashSet::new();

    let mut common_unis: Vec<char> = font_a.unicode_to_id.keys()
        .filter(|u| font_b.unicode_to_id.contains_key(u))
        .cloned().collect();
    common_unis.sort_unstable();

    for uni in common_unis {
        let gid_a = font_a.unicode_to_id[&uni];
        let gid_b = font_b.unicode_to_id[&uni];
        processed_a.insert(gid_a);
        processed_b.insert(gid_b);

        let name_a = font_a.id_to_name.get(&gid_a).cloned().unwrap_or_else(|| format!("gid{}", gid_a.to_u32()));
        let name_b = font_b.id_to_name.get(&gid_b).cloned().unwrap_or_else(|| format!("gid{}", gid_b.to_u32()));

        if name_a == name_b {
            if font_a.hashes.get(&gid_a) != font_b.hashes.get(&gid_b) {
                result.changed.push(name_a);
            }
        } else {
            if font_a.hashes.get(&gid_a) != font_b.hashes.get(&gid_b) {
                result.modified.push((name_a, name_b));
            } else {
                result.renamed.push((name_a, name_b));
            }
        }
    }

    let remaining_a: Vec<GlyphId> = font_a.glyph_ids.iter()
        .filter(|gid| !processed_a.contains(gid)).cloned().collect();

    let mut b_hash_map: HashMap<&str, Vec<GlyphId>> = HashMap::new();
    for gid_b in font_b.glyph_ids.iter().filter(|gid| !processed_b.contains(gid)) {
        if let Some(h) = font_b.hashes.get(gid_b) {
            b_hash_map.entry(h).or_default().push(*gid_b);
        }
    }

    let mut matched_b = HashSet::new();

    for gid_a in remaining_a {
        let name_a = font_a.id_to_name.get(&gid_a).cloned().unwrap_or_else(|| format!("gid{}", gid_a.to_u32()));

        let mut found = false;
        if let Some(h_a) = font_a.hashes.get(&gid_a) {
            if let Some(gids_b) = b_hash_map.get_mut(h_a.as_str()) {
                if let Some(gid_b) = gids_b.pop() {
                    let name_b = font_b.id_to_name.get(&gid_b).cloned().unwrap_or_else(|| format!("gid{}", gid_b.to_u32()));
                    result.renamed.push((name_a.clone(), name_b));
                    matched_b.insert(gid_b);
                    processed_a.insert(gid_a);
                    processed_b.insert(gid_b);
                    found = true;
                }
            }
        }

        if !found {
            result.removed.push(name_a);
        }
    }

    for gid_b in font_b.glyph_ids.iter().filter(|gid| !processed_b.contains(gid) && !matched_b.contains(gid)) {
        let name_b = font_b.id_to_name.get(gid_b).cloned().unwrap_or_else(|| format!("gid{}", gid_b.to_u32()));
        result.added.push(name_b);
    }

    result.added.sort_unstable();
    result.removed.sort_unstable();
    result.changed.sort_unstable();
    result.renamed.sort_unstable();
    result.modified.sort_unstable();

    result
}

pub fn glyph_to_char(font_info: &FontInfo, glyph_name: &str) -> Option<char> {
    if let Some(&gid) = font_info.name_to_id.get(glyph_name) {
        font_info.id_to_unicode.get(&gid).copied()
    } else {
        None
    }
}

fn data_to_base64(data: &[u8]) -> String {
    general_purpose::STANDARD.encode(data)
}

fn render_row(
    tag: &str,
    tag_class: &str,
    name: &str,
    char: Option<char>,
    content_html: &str,
    sub_info: Option<&str>,
) -> String {
    let name_display = if let Some(sub) = sub_info {
        format!("<strong>{}</strong><br/>&#8594;<br/><strong>{}</strong>", name, sub)
    } else {
        format!("<strong>{}</strong>", name)
    };

    let uni_info = if let Some(c) = char {
        format!("U+{:04X}", c as u32)
    } else {
        String::new()
    };

    format!(
        "<div class='row'>\n\
         <div class='meta'>\n\
         <span class='tag {}'>{}</span><br/>{}<br/>{}</div>\n\
         {}\n\
         </div>\n",
        tag_class, tag, name_display, uni_info, content_html
    )
}

pub fn generate_html_string(
    old_data: &[u8],
    new_data: &[u8],
    font_a: &FontInfo,
    font_b: &FontInfo,
    diff: &FontDiff,
) -> String {
    let b64_a = data_to_base64(old_data);
    let b64_b = data_to_base64(new_data);
    let mime = "font/ttf";

    let mut html = String::new();

    html.push_str("<!doctype html>\n");
    html.push_str("<html><head><meta charset='utf-8'>\n");
    html.push_str("<style>\n");
    html.push_str(&format!(
        r#"
@font-face {{ font-family: "FontA"; src: url(data:{};base64,{}); }}
@font-face {{ font-family: "FontB"; src: url(data:{};base64,{}); }}

body {{ font-family: sans-serif; padding: 20px; }}
h2 {{ margin-top: 32px; border-bottom: 2px solid #eee; padding-bottom: 8px; clear: both; }}

.section {{
  margin: 12px 0 24px;
  padding: 12px;
  border: 1px solid #ddd;
  border-radius: 6px;
  background: #fcfcfc;
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}}

.section.modified {{ border-color: #ffcc80; background: #fff8f0; }}
.section.renamed {{ border-color: #e6ccff; background: #fdfaff; }}
.section.changed {{ border-color: #f2bcbc; background: #fff0f0; }}
.section.added   {{ border-color: #bde5bd; }}
.section.removed {{ border-color: #ddd; }}

.row {{
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  margin: 4px;
  width: 110px;
  padding: 8px;
  border-radius: 4px;
}}
.row:hover {{ background: rgba(0,0,0,0.04); }}

.meta {{
  font-size: 10px;
  font-family: monospace;
  color: #555;
  line-height: 1.2;
  text-align: center;
  word-break: break-all;
}}

.glyph {{
  font-size: 80px;
  position: relative;
  line-height: 1;
  height: 80px;
  width: 100%;
  text-align: center;
}}

.old {{ color: rgba(255, 0, 0, 0.6); font-family: "FontA"; position: absolute; left: 0; top: 0; width: 100%; }}
.new {{ color: rgba(0, 0, 255, 0.6); font-family: "FontB"; position: absolute; left: 0; top: 0; width: 100%; }}

.modified .old {{ color: rgba(255, 69, 0, 0.6); }}
.modified .new {{ color: rgba(30, 144, 255, 0.8); z-index: 2; }}

.added {{ color: green; font-family: "FontB"; }}
.removed {{ color: #999; font-family: "FontA"; }}
.renamed-char {{ color: purple; font-family: "FontA"; }}

.tag {{
  display: inline-block;
  font-size: 9px;
  padding: 2px 4px;
  border-radius: 3px;
  font-weight: bold;
  margin-bottom: 4px;
}}
.tag.modified {{ background: #ffe0b2; color: #e65100; }}
.tag.renamed {{ background: #e6ccff; color: #8a2be2; }}
.tag.added   {{ background: #e6ffe6; color: #0a0; }}
.tag.removed {{ background: #f2f2f2; color: #666; }}
.tag.changed {{ background: #ffcdd2; color: #c00; }}
"#,
        mime, b64_a, mime, b64_b
    ));
    html.push_str("</style></head><body>\n");
    html.push_str("<h1>Font Diff</h1>\n");

    if !diff.modified.is_empty() {
        html.push_str("<h2>Modified (Renamed & Changed)</h2>\n");
        html.push_str("<div class='section modified'>\n");
        for (name_a, name_b) in &diff.modified {
            if let Some(char) = glyph_to_char(font_a, name_a) {
                let content = format!(
                    "<div class='glyph'><span class='old'>{}</span><span class='new'>{}</span></div>",
                    char, char
                );
                html.push_str(&render_row("MODIFIED", "modified", name_a, Some(char), &content, Some(name_b)));
            }
        }
        html.push_str("</div>\n");
    }

    if !diff.changed.is_empty() {
        html.push_str("<h2>Changed</h2>\n");
        html.push_str("<div class='section changed'>\n");
        for g in &diff.changed {
            let char = glyph_to_char(font_a, g).or_else(|| glyph_to_char(font_b, g));
            if let Some(c) = char {
                let content = format!(
                    "<div class='glyph'><span class='old'>{}</span><span class='new'>{}</span></div>",
                    c, c
                );
                html.push_str(&render_row("CHANGED", "changed", g, Some(c), &content, None));
            }
        }
        html.push_str("</div>\n");
    }

    if !diff.renamed.is_empty() {
        html.push_str("<h2>Renamed (Only)</h2>\n");
        html.push_str("<div class='section renamed'>\n");
        for (name_a, name_b) in &diff.renamed {
            let char = glyph_to_char(font_a, name_a).or_else(|| glyph_to_char(font_b, name_b));
            if let Some(c) = char {
                let content = format!("<div class='glyph'><span class='renamed-char'>{}</span></div>", c);
                html.push_str(&render_row("RENAMED", "renamed", name_a, Some(c), &content, Some(name_b)));
            }
        }
        html.push_str("</div>\n");
    }

    if !diff.added.is_empty() {
        html.push_str("<h2>Added</h2>\n");
        html.push_str("<div class='section added'>\n");
        for g in &diff.added {
            if let Some(c) = glyph_to_char(font_b, g) {
                let content = format!("<div class='glyph'><span class='added'>{}</span></div>", c);
                html.push_str(&render_row("ADDED", "added", g, Some(c), &content, None));
            }
        }
        html.push_str("</div>\n");
    }

    if !diff.removed.is_empty() {
        html.push_str("<h2>Removed</h2>\n");
        html.push_str("<div class='section removed'>\n");
        for g in &diff.removed {
            if let Some(c) = glyph_to_char(font_a, g) {
                let content = format!("<div class='glyph'><span class='removed'>{}</span></div>", c);
                html.push_str(&render_row("REMOVED", "removed", g, Some(c), &content, None));
            }
        }
        html.push_str("</div>\n");
    }

    html.push_str("</body></html>\n");

    html
}

pub fn run_diff_core(old_data: &[u8], new_data: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    let font_a_ref = FontRef::new(old_data)?;
    let font_b_ref = FontRef::new(new_data)?;

    let font_a_info = init_font_info(font_a_ref);
    let font_b_info = init_font_info(font_b_ref);

    let diff = diff_fonts(&font_a_info, &font_b_info);

    let html = generate_html_string(old_data, new_data, &font_a_info, &font_b_info, &diff);

    Ok(html)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn diff_fonts_wasm(old_data: &[u8], new_data: &[u8]) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();
    
    run_diff_core(old_data, new_data)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
