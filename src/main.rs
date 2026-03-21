use std::path::PathBuf;
use std::fs;
use std::time::Instant;
use clap::Parser;
use base64::Engine as _;
use font_diff::*;

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

fn data_to_base64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
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
    let old_data = fs::read(old_path)?;
    let new_data = fs::read(new_path)?;
    let b64_a = data_to_base64(&old_data);
    let b64_b = data_to_base64(&new_data);
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

    let t0 = Instant::now();
    let font_a_data = fs::read(&args.old)?;
    let font_b_data = fs::read(&args.new)?;
    println!("[1] Read files: {:.3}s ({} + {})", t0.elapsed().as_secs_f32(), args.old.display(), args.new.display());

    let t1 = Instant::now();
    let font_a_ref = skrifa::font::FontRef::new(&font_a_data)?;
    let font_b_ref = skrifa::font::FontRef::new(&font_b_data)?;
    println!("[2] Parse font headers: {:.3}s", t1.elapsed().as_secs_f32());

    let t2 = Instant::now();
    let font_a_info = init_font_info(font_a_ref);
    println!("    Font A: {} glyphs, {} unicode mappings", font_a_info.glyph_ids.len(), font_a_info.unicode_to_id.len());
    println!("[3] Init font A info: {:.3}s", t2.elapsed().as_secs_f32());

    let t3 = Instant::now();
    let font_b_info = init_font_info(font_b_ref);
    println!("    Font B: {} glyphs, {} unicode mappings", font_b_info.glyph_ids.len(), font_b_info.unicode_to_id.len());
    println!("[4] Init font B info: {:.3}s", t3.elapsed().as_secs_f32());

    let t4 = Instant::now();
    let diff = diff_fonts(&font_a_info, &font_b_info);
    println!("[5] Diff fonts: {:.3}s", t4.elapsed().as_secs_f32());

    let t5 = Instant::now();
    generate_html(&args.old, &args.new, &font_a_info, &font_b_info, &diff, &args.output)?;
    println!("[6] Generate HTML: {:.3}s", t5.elapsed().as_secs_f32());

    println!("\nTotal: {:.3}s", t0.elapsed().as_secs_f32());
    println!("\nSummary:");
    println!("  Modified (Renamed + Changed): {}", diff.modified.len());
    println!("  Changed (Same Name): {}", diff.changed.len());
    println!("  Renamed (Same Shape): {}", diff.renamed.len());
    println!("  Added: {}", diff.added.len());
    println!("  Removed: {}", diff.removed.len());
    println!("Output: {}", args.output.display());

    Ok(())
}
