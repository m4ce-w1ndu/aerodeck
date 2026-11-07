use eframe::egui::{self, Color32, RichText, Ui};
use std::collections::HashMap;

use crate::gui::backend::GuiCommand;

pub struct LvarViewState {
    pub search_query: String,
    pub manual_lvar_name: String,
    pub manual_lvar_val: String,
    pub rpn_code: String,
}

impl Default for LvarViewState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            manual_lvar_name: "XMLVAR_Pitot_Heat_State".to_string(),
            manual_lvar_val: "1.0".to_string(),
            rpn_code: "(>K:TOGGLE_MASTER_BATTERY)".to_string(),
        }
    }
}

pub fn render_lvar_view(
    ui: &mut Ui,
    state: &mut LvarViewState,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    lvars: &[String],
    live_values: &HashMap<String, f64>,
    bridge_active: bool,
) {
    ui.heading(RichText::new("MSFS WASM LVAR Bridge & Variable Explorer").color(Color32::from_rgb(100, 200, 255)));
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        let status_color = if bridge_active { Color32::from_rgb(0, 255, 120) } else { Color32::LIGHT_RED };
        ui.label(RichText::new(if bridge_active { "● Bridge Active" } else { "○ Bridge Disconnected" }).color(status_color).strong());
        ui.label(format!("(Total Variables Discovered: {})", lvars.len()));

        if ui.button("🔄 Scan & Refresh LVARs").clicked() {
            let _ = cmd_tx.send(GuiCommand::RequestLvarScan);
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Manual test & Calculator console
    ui.group(|ui| {
        ui.label(RichText::new("Direct Variable Write & RPN Execution").strong());

        ui.horizontal(|ui| {
            ui.label("LVAR:");
            ui.text_edit_singleline(&mut state.manual_lvar_name);
            ui.label("Value:");
            ui.text_edit_singleline(&mut state.manual_lvar_val);
            if ui.button("Set Value").clicked() {
                if let Ok(v) = state.manual_lvar_val.parse::<f64>() {
                    let _ = cmd_tx.send(GuiCommand::SetLvar {
                        name: state.manual_lvar_name.clone(),
                        value: v,
                    });
                }
            }
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Execute RPN:");
            ui.text_edit_singleline(&mut state.rpn_code);
            if ui.button("Run Calculator Code").clicked() {
                let _ = cmd_tx.send(GuiCommand::ExecCalculatorCode {
                    code: state.rpn_code.clone(),
                });
            }
        });
    });

    ui.add_space(8.0);

    // Search filter
    ui.horizontal(|ui| {
        ui.label("Search Variables:");
        ui.text_edit_singleline(&mut state.search_query);
        if ui.button("Clear").clicked() {
            state.search_query.clear();
        }
    });

    ui.add_space(4.0);

    // Variable list table
    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
        egui::Grid::new("lvar_table_grid")
            .striped(true)
            .min_col_width(200.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Variable Name").strong());
                ui.label(RichText::new("Current Value").strong());
                ui.label(RichText::new("Action").strong());
                ui.end_row();

                for lvar in lvars {
                    if !state.search_query.is_empty()
                        && !lvar.to_lowercase().contains(&state.search_query.to_lowercase())
                    {
                        continue;
                    }

                    let val_str = live_values
                        .get(lvar)
                        .map(|v| format!("{:.4}", v))
                        .unwrap_or_else(|| "--".to_string());

                    ui.label(lvar);
                    ui.label(RichText::new(val_str).color(Color32::from_rgb(0, 220, 255)));

                    ui.horizontal(|ui| {
                        if ui.small_button("Select").clicked() {
                            state.manual_lvar_name = lvar.clone();
                        }
                    });

                    ui.end_row();
                }
            });
    });
}
