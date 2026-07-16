use clap::Parser;
use font_diff::{diff_fonts, generate_html_report, init_font_info, HtmlOptions};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    ensure_exists(&args.old, "Old")?;
    ensure_exists(&args.new, "New")?;

    let started = Instant::now();
    let font_a_data = fs::read(&args.old)?;
    let font_b_data = fs::read(&args.new)?;
    println!("[1] Read files: {:.3}s", started.elapsed().as_secs_f32());

    let parsed = Instant::now();
    let font_a_ref = skrifa::font::FontRef::new(&font_a_data)?;
    let font_b_ref = skrifa::font::FontRef::new(&font_b_data)?;
    println!(
        "[2] Parse font headers: {:.3}s",
        parsed.elapsed().as_secs_f32()
    );

    let step = Instant::now();
    let font_a = init_font_info(font_a_ref);
    println!("[3] Init font A: {:.3}s", step.elapsed().as_secs_f32());
    let step = Instant::now();
    let font_b = init_font_info(font_b_ref);
    println!("[4] Init font B: {:.3}s", step.elapsed().as_secs_f32());

    let step = Instant::now();
    let diff = diff_fonts(&font_a, &font_b);
    println!("[5] Diff fonts: {:.3}s", step.elapsed().as_secs_f32());

    let step = Instant::now();
    let old_label = args.old.display().to_string();
    let new_label = args.new.display().to_string();
    let html = generate_html_report(
        &font_a_data,
        &font_b_data,
        &font_a,
        &font_b,
        &diff,
        HtmlOptions {
            old_mime: mime_type(&args.old),
            new_mime: mime_type(&args.new),
            old_label: Some(&old_label),
            new_label: Some(&new_label),
        },
    );
    fs::write(&args.output, html)?;
    println!("[6] Generate HTML: {:.3}s", step.elapsed().as_secs_f32());

    println!("\nTotal: {:.3}s", started.elapsed().as_secs_f32());
    println!("\nSummary:");
    println!("  Modified (Renamed + Changed): {}", diff.modified.len());
    println!("  Changed (Same Name): {}", diff.changed.len());
    println!("  Renamed (Same Shape): {}", diff.renamed.len());
    println!("  Added: {}", diff.added.len());
    println!("  Removed: {}", diff.removed.len());
    println!("Output: {}", args.output.display());
    Ok(())
}

fn ensure_exists(path: &Path, label: &str) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!("{label} font file not found: {}", path.display()).into())
    }
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("otf") => "font/otf",
        _ => "font/ttf",
    }
}
