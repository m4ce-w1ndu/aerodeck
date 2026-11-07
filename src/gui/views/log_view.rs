use eframe::egui::{self, Color32, RichText, Ui};

pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub text: String,
}

pub fn render_log_view(ui: &mut Ui, logs: &[LogEntry], auto_scroll: &mut bool) {
    ui.horizontal(|ui| {
        ui.heading(RichText::new("Live Event & Communication Log").color(Color32::from_rgb(180, 220, 255)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(auto_scroll, "Auto-scroll to bottom");
        });
    });

    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .max_height(480.0)
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for entry in logs {
                let level_color = match entry.level.as_str() {
                    "ERROR" => Color32::from_rgb(255, 80, 80),
                    "WARN" => Color32::from_rgb(255, 180, 50),
                    "ACTION" => Color32::from_rgb(0, 255, 150),
                    "INPUT" => Color32::from_rgb(255, 220, 0),
                    _ => Color32::from_rgb(150, 190, 255),
                };

                ui.horizontal(|ui| {
                    ui.label(RichText::new(&entry.timestamp).color(Color32::GRAY).monospace());
                    ui.label(RichText::new(format!("[{:^6}]", entry.level)).color(level_color).monospace().strong());
                    ui.label(RichText::new(&entry.text));
                });
            }

            if *auto_scroll {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
            }
        });
}
