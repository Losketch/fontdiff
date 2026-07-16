mod diff;
mod font;
mod hash;
mod html;

pub use diff::diff_fonts;
pub use font::{glyph_to_char, init_font_info, FontInfo};
pub use html::{generate_html_report, generate_html_string, HtmlOptions};

#[derive(Debug, Default)]
pub struct FontDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub renamed: Vec<(String, String)>,
    pub modified: Vec<(String, String)>,
}

pub fn run_diff_core(
    old_data: &[u8],
    new_data: &[u8],
) -> Result<String, Box<dyn std::error::Error>> {
    let font_a = init_font_info(skrifa::font::FontRef::new(old_data)?);
    let font_b = init_font_info(skrifa::font::FontRef::new(new_data)?);
    let diff = diff_fonts(&font_a, &font_b);

    Ok(generate_html_string(
        old_data, new_data, &font_a, &font_b, &diff,
    ))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::run_diff_core;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn diff_fonts_wasm(old_data: &[u8], new_data: &[u8]) -> Result<String, JsValue> {
        console_error_panic_hook::set_once();
        run_diff_core(old_data, new_data).map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::diff_fonts_wasm;
