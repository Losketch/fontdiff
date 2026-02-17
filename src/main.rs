use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::fs;
use base64::{engine::general_purpose, Engine as _};
use clap::Parser;
use read_fonts::{types::GlyphId, TableProvider};
use sha2::{Digest, Sha256};
use skrifa::{
    charmap::Charmap,
    font::FontRef,
    GlyphNames,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen, pen::PathElement},
    MetadataProvider,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Old font file (baseline)
    old: PathBuf,
    /// New font file (target)
    new: PathBuf,
    /// Output HTML filename
    #[arg(short, long, default_value = "diff.html")]
    output: PathBuf,
}

struct RecordingPen {
    elements: Vec<PathElement>,
}

impl RecordingPen {
    fn new() -> Self {
        Self {
            elements: Vec::new(),
        }
    }
}

impl OutlinePen for RecordingPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.elements.push(PathElement::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.elements.push(PathElement::LineTo { x, y });
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.elements.push(PathElement::QuadTo {
            cx0,
            cy0,
            x,
            y,
        });
    }

    fn curve_to(
        &mut self,
        cx0: f32,
        cy0: f32,
        cx1: f32,
        cy1: f32,
        x: f32,
        y: f32,
    ) {
        self.elements.push(PathElement::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.elements.push(PathElement::Close);
    }
}

fn normalize_pen_value(pen_value: &RecordingPen, precision: u32) -> Vec<String> {
    pen_value
        .elements
        .iter()
        .map(|elem| match elem {
            PathElement::MoveTo { x, y } => {
                format!("M{{{:.prec$},{:.prec$}}}", x, y, prec = precision as usize)
            }
            PathElement::LineTo { x, y } => {
                format!("L{{{:.prec$},{:.prec$}}}", x, y, prec = precision as usize)
            }
            PathElement::QuadTo { cx0, cy0, x, y } => {
                format!("Q{{{:.prec$},{:.prec$},{:.prec$},{:.prec$}}}", cx0, cy0, x, y, prec = precision as usize)
            }
            PathElement::CurveTo {
                cx0,
                cy0,
                cx1,
                cy1,
                x,
                y,
            } => {
                format!("C{{{:.prec$},{:.prec$},{:.prec$},{:.prec$},{:.prec$},{:.prec$}}}", cx0, cy0, cx1, cy1, x, y, prec = precision as usize)
            }
            PathElement::Close => "Z".to_string(),
        })
        .collect()
}

fn glyph_hash(font: &FontRef, glyph_id: GlyphId) -> Option<String> {
    let mut pen = RecordingPen::new();
    let outlines = font.outline_glyphs();

    if let Some(glyph) = outlines.get(glyph_id) {
        let location = LocationRef::default();
        let settings = DrawSettings::unhinted(Size::unscaled(), location);
        if glyph.draw(settings, &mut pen).is_err() {
            return None;
        }
    }

    let width: u16 = font
        .hmtx()
        .ok()
        .and_then(|hmtx| hmtx.advance(glyph_id))
        .unwrap_or(0);

    let normalized = normalize_pen_value(&pen, 2);
    let data = format!("{:?}", normalized);

    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hasher.update(width.to_be_bytes());

    Some(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Default)]
struct FontDiff {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<String>,
    renamed: Vec<(String, String)>,
    modified: Vec<(String, String)>,
}

struct FontInfo<'a> {
    font: FontRef<'a>,
    charmap: Charmap<'a>,
    glyph_names: GlyphNames<'a>,
    glyph_ids: Vec<GlyphId>,
    name_to_id: HashMap<String, GlyphId>,
    id_to_name: HashMap<GlyphId, String>,
    unicode_to_id: HashMap<char, GlyphId>,
    id_to_unicode: HashMap<GlyphId, char>,
    hashes: HashMap<GlyphId, String>,
}

fn init_font_info(font: FontRef) -> FontInfo {
    let charmap = Charmap::new(&font);
    let glyph_names = GlyphNames::new(&font);
    let num_glyphs = font
        .maxp()
        .map(|m| m.num_glyphs() as usize)
        .unwrap_or(0);

    let glyph_ids: Vec<GlyphId> = (0..num_glyphs).map(|i| GlyphId::new(i as u32)).collect();

    let mut name_to_id = HashMap::new();
    let mut id_to_name = HashMap::new();

    for &gid in &glyph_ids {
        if let Some(name) = glyph_names.get(gid) {
            let name_str = name.as_str().to_string();
            name_to_id.insert(name_str.clone(), gid);
            id_to_name.insert(gid, name_str);
        }
    }

    let mut unicode_to_id = HashMap::new();
    let mut id_to_unicode = HashMap::new();

    for mapping in charmap.mappings() {
        let (codepoint, gid) = mapping;
        if let Some(ch) = std::char::from_u32(codepoint) {
            unicode_to_id.insert(ch, gid);
            id_to_unicode.insert(gid, ch);
        }
    }

    let mut hashes = HashMap::new();
    for &gid in &glyph_ids {
        if let Some(hash) = glyph_hash(&font, gid) {
            hashes.insert(gid, hash);
        }
    }

    FontInfo {
        font,
        charmap,
        glyph_names,
        glyph_ids,
        name_to_id,
        id_to_name,
        unicode_to_id,
        id_to_unicode,
        hashes,
    }
}

fn diff_fonts(font_a: &FontInfo, font_b: &FontInfo) -> FontDiff {
    let mut result = FontDiff::default();

    let glyphs_a: HashSet<GlyphId> = font_a.glyph_ids.iter().cloned().collect();
    let glyphs_b: HashSet<GlyphId> = font_b.glyph_ids.iter().cloned().collect();

    let mut processed_a: HashSet<GlyphId> = HashSet::new();
    let mut processed_b: HashSet<GlyphId> = HashSet::new();

    let common_unis: HashSet<char> = font_a
        .unicode_to_id
        .keys()
        .cloned()
        .collect::<HashSet<_>>()
        .intersection(&font_b.unicode_to_id.keys().cloned().collect())
        .cloned()
        .collect();

    let mut common_unis_vec: Vec<char> = common_unis.iter().cloned().collect();
    common_unis_vec.sort();

    for &uni in &common_unis_vec {
        let gid_a = font_a.unicode_to_id.get(&uni).copied().unwrap();
        let gid_b = font_b.unicode_to_id.get(&uni).copied().unwrap();

        let name_a = font_a.id_to_name.get(&gid_a).cloned().unwrap_or_else(|| format!("gid{}", gid_a.to_u32()));
        let name_b = font_b.id_to_name.get(&gid_b).cloned().unwrap_or_else(|| format!("gid{}", gid_b.to_u32()));

        let hash_a = font_a.hashes.get(&gid_a);
        let hash_b = font_b.hashes.get(&gid_b);

        processed_a.insert(gid_a);
        processed_b.insert(gid_b);

        if name_a == name_b {
            if hash_a != hash_b {
                result.changed.push(name_a);
            }
        } else {
            match (hash_a, hash_b) {
                (Some(ha), Some(hb)) if ha != hb => {
                    result.modified.push((name_a, name_b));
                }
                (Some(ha), Some(hb)) if ha == hb => {
                    result.renamed.push((name_a, name_b));
                }
                _ => {}
            }
        }
    }

    let remaining_a: HashSet<GlyphId> = &glyphs_a - &processed_a;
    let remaining_b: HashSet<GlyphId> = &glyphs_b - &processed_b;

    let mut used_b_in_rename: HashSet<GlyphId> = HashSet::new();

    for gid_a in &remaining_a {
        let hash_a = font_a.hashes.get(gid_a);

        if let Some(ha) = hash_a {
            let mut match_found = None;

            for gid_b in &remaining_b {
                if used_b_in_rename.contains(gid_b) {
                    continue;
                }

                if let Some(hb) = font_b.hashes.get(gid_b) {
                    if ha == hb {
                        match_found = Some(*gid_b);
                        break;
                    }
                }
            }

            if let Some(gid_b) = match_found {
                let name_a = font_a.id_to_name.get(gid_a).cloned().unwrap_or_else(|| format!("gid{}", gid_a.to_u32()));
                let name_b = font_b.id_to_name.get(&gid_b).cloned().unwrap_or_else(|| format!("gid{}", gid_b.to_u32()));
                result.renamed.push((name_a, name_b));
                used_b_in_rename.insert(gid_b);
            }
        }
    }

    let renamed_b_names: HashSet<String> = result
        .renamed
        .iter()
        .map(|(_, b)| b.clone())
        .chain(result.modified.iter().map(|(_, b)| b.clone()))
        .collect();
    let renamed_a_names: HashSet<String> = result
        .renamed
        .iter()
        .map(|(a, _)| a.clone())
        .chain(result.modified.iter().map(|(a, _)| a.clone()))
        .collect();

    result.added = remaining_b
        .iter()
        .filter_map(|gid| font_b.id_to_name.get(gid))
        .filter(|name| !renamed_b_names.contains(*name))
        .cloned()
        .collect();

    result.removed = remaining_a
        .iter()
        .filter_map(|gid| font_a.id_to_name.get(gid))
        .filter(|name| !renamed_a_names.contains(*name))
        .cloned()
        .collect();

    result.added.sort();
    result.removed.sort();
    result.changed.sort();
    result.renamed.sort();
    result.modified.sort();

    result
}

fn glyph_to_char(font_info: &FontInfo, glyph_name: &str) -> Option<char> {
    if let Some(&gid) = font_info.name_to_id.get(glyph_name) {
        font_info.id_to_unicode.get(&gid).copied()
    } else {
        None
    }
}

fn file_to_base64(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    Ok(general_purpose::STANDARD.encode(&data))
}

fn get_mime_type(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("otf") => "font/otf",
        _ => "font/ttf",
    }
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

fn generate_html(
    old_path: &PathBuf,
    new_path: &PathBuf,
    font_a: &FontInfo,
    font_b: &FontInfo,
    diff: &FontDiff,
    output: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let b64_a = file_to_base64(old_path)?;
    let b64_b = file_to_base64(new_path)?;
    let mime = get_mime_type(old_path);

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
    html.push_str(&format!(
        "<p>Old: {}<br/>New: {}</p>\n",
        old_path.display(),
        new_path.display()
    ));

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

    fs::write(output, html)?;

    Ok(())
}
 
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if !args.old.exists() {
        eprintln!("Error: Old font file not found: {}", args.old.display());
        std::process::exit(1);
    }
    if !args.new.exists() {
        eprintln!("Error: New font file not found: {}", args.new.display());
        std::process::exit(1);
    }

    let font_a_data = fs::read(&args.old)?;
    let font_b_data = fs::read(&args.new)?;

    let font_a_ref = FontRef::new(&font_a_data)?;
    let font_b_ref = FontRef::new(&font_b_data)?;

    let font_a_info = init_font_info(font_a_ref);
    let font_b_info = init_font_info(font_b_ref);

    let diff = diff_fonts(&font_a_info, &font_b_info);

    generate_html(&args.old, &args.new, &font_a_info, &font_b_info, &diff, &args.output)?;

    println!("Summary:");
    println!("  Modified (Renamed + Changed): {}", diff.modified.len());
    println!("  Changed (Same Name): {}", diff.changed.len());
    println!("  Renamed (Same Shape): {}", diff.renamed.len());
    println!("  Added: {}", diff.added.len());
    println!("  Removed: {}", diff.removed.len());
    println!("Output: {}", args.output.display());

    Ok(())
}
