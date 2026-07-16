use crate::{glyph_to_char, FontDiff, FontInfo};
use base64::{engine::general_purpose, Engine as _};
use std::fmt::Write;

#[derive(Clone, Copy)]
pub struct HtmlOptions<'a> {
    pub old_mime: &'a str,
    pub new_mime: &'a str,
    pub old_label: Option<&'a str>,
    pub new_label: Option<&'a str>,
}

impl Default for HtmlOptions<'static> {
    fn default() -> Self {
        Self {
            old_mime: "font/ttf",
            new_mime: "font/ttf",
            old_label: None,
            new_label: None,
        }
    }
}

pub fn generate_html_string(
    old_data: &[u8],
    new_data: &[u8],
    font_a: &FontInfo,
    font_b: &FontInfo,
    diff: &FontDiff,
) -> String {
    generate_html_report(
        old_data,
        new_data,
        font_a,
        font_b,
        diff,
        HtmlOptions::default(),
    )
}

pub fn generate_html_report(
    old_data: &[u8],
    new_data: &[u8],
    font_a: &FontInfo,
    font_b: &FontInfo,
    diff: &FontDiff,
    options: HtmlOptions<'_>,
) -> String {
    let encoded_size = (old_data.len() + new_data.len()) * 4 / 3;
    let mut html = String::with_capacity(encoded_size + 16 * 1024);
    html.push_str("<!doctype html><html><head><meta charset='utf-8'><style>\n@font-face{font-family:FontA;src:url(data:");
    html.push_str(options.old_mime);
    html.push_str(";base64,");
    general_purpose::STANDARD.encode_string(old_data, &mut html);
    html.push_str(")}\n@font-face{font-family:FontB;src:url(data:");
    html.push_str(options.new_mime);
    html.push_str(";base64,");
    general_purpose::STANDARD.encode_string(new_data, &mut html);
    html.push_str(")}\n");
    html.push_str(include_str!("report.css"));
    html.push_str("</style></head><body><h1>Font Diff</h1>\n");

    if let (Some(old), Some(new)) = (options.old_label, options.new_label) {
        let _ = writeln!(html, "<p>Old: {}<br>New: {}</p>", escape(old), escape(new));
    }

    render_pairs(
        &mut html,
        "Modified (Renamed & Changed)",
        "modified",
        "MODIFIED",
        &diff.modified,
        |old, _| glyph_to_char(font_a, old),
        "overlay",
    );
    render_names(
        &mut html,
        "Changed",
        "changed",
        "CHANGED",
        &diff.changed,
        |name| glyph_to_char(font_a, name).or_else(|| glyph_to_char(font_b, name)),
    );
    render_pairs(
        &mut html,
        "Renamed (Only)",
        "renamed",
        "RENAMED",
        &diff.renamed,
        |old, new| glyph_to_char(font_a, old).or_else(|| glyph_to_char(font_b, new)),
        "renamed-char",
    );
    render_names(&mut html, "Added", "added", "ADDED", &diff.added, |name| {
        glyph_to_char(font_b, name)
    });
    render_names(
        &mut html,
        "Removed",
        "removed",
        "REMOVED",
        &diff.removed,
        |name| glyph_to_char(font_a, name),
    );
    html.push_str("</body></html>\n");
    html
}

fn render_names<F>(
    html: &mut String,
    heading: &str,
    class: &str,
    tag: &str,
    names: &[String],
    find_char: F,
) where
    F: Fn(&str) -> Option<char>,
{
    if names.is_empty() {
        return;
    }
    section_start(html, heading, class);
    for name in names {
        if let Some(ch) = find_char(name) {
            row(html, tag, class, name, None, ch, class);
        }
    }
    html.push_str("</div>\n");
}

fn render_pairs<F>(
    html: &mut String,
    heading: &str,
    class: &str,
    tag: &str,
    pairs: &[(String, String)],
    find_char: F,
    glyph_class: &str,
) where
    F: Fn(&str, &str) -> Option<char>,
{
    if pairs.is_empty() {
        return;
    }
    section_start(html, heading, class);
    for (old, new) in pairs {
        if let Some(ch) = find_char(old, new) {
            row(html, tag, class, old, Some(new), ch, glyph_class);
        }
    }
    html.push_str("</div>\n");
}

fn section_start(html: &mut String, heading: &str, class: &str) {
    let _ = writeln!(html, "<h2>{heading}</h2><div class='section {class}'>");
}

fn row(
    html: &mut String,
    tag: &str,
    class: &str,
    name: &str,
    new_name: Option<&str>,
    ch: char,
    glyph_class: &str,
) {
    let _ = write!(
        html,
        "<div class='row'><div class='meta'><span class='tag {class}'>{tag}</span><br><strong>{}</strong>",
        escape(name)
    );
    if let Some(new_name) = new_name {
        let _ = write!(html, "<br>&#8594;<br><strong>{}</strong>", escape(new_name));
    }
    let _ = write!(html, "<br>U+{:04X}</div><div class='glyph'>", ch as u32);
    let escaped = escape(&ch.to_string());
    if glyph_class == "overlay" || class == "changed" {
        let _ = write!(
            html,
            "<span class='old'>{escaped}</span><span class='new'>{escaped}</span>"
        );
    } else {
        let _ = write!(html, "<span class='{glyph_class}'>{escaped}</span>");
    }
    html.push_str("</div></div>\n");
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn escapes_report_text() {
        assert_eq!(escape("<&\"'>"), "&lt;&amp;&quot;&#39;&gt;");
    }
}
