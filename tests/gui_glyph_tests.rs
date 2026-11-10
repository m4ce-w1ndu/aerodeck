//! Every non-ASCII character used in GUI source must have a glyph in the fonts the app
//! loads, otherwise egui draws an empty box.

use eframe::egui;

fn gui_chars() -> Vec<(char, String)> {
    let mut out: Vec<(char, String)> = Vec::new();
    let mut stack = vec![std::path::PathBuf::from("src/gui")];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                for ch in std::fs::read_to_string(&p).unwrap().chars() {
                    if !ch.is_ascii() && !out.iter().any(|(c, _)| *c == ch) {
                        out.push((ch, p.display().to_string()));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn gui_glyphs_exist_in_fonts() {
    let ctx = egui::Context::default();
    aerodeck::gui::theme::install_fonts(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    let font = egui::FontId::proportional(14.0);
    let missing: Vec<String> = gui_chars()
        .into_iter()
        .filter(|(c, _)| !ctx.fonts(|f| f.has_glyph(&font, *c)))
        .map(|(c, file)| format!("U+{:04X} '{}' ({})", c as u32, c, file))
        .collect();
    assert!(missing.is_empty(), "characters without a glyph:\n{}", missing.join("\n"));
}
