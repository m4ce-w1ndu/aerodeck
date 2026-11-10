//! Variables view: PMDG SDK status, SimConnect SimVars and WASM-bridge LVARs.

use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};
use std::collections::HashMap;

use crate::gui::backend::{GuiCommand, PmdgUiStatus};
use crate::gui::theme::AeroTheme;
use crate::gui::widgets::{ui_card, ui_status_badge};

pub struct LvarViewState {
    pub search_query: String,
    pub manual_lvar_name: String,
    pub manual_lvar_val: String,
    pub rpn_code: String,
    pub simvar_name: String,
    pub simvar_value: String,
}

impl Default for LvarViewState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            manual_lvar_name: "XMLVAR_Pitot_Heat_State".to_string(),
            manual_lvar_val: "1.0".to_string(),
            rpn_code: "(>K:TOGGLE_MASTER_BATTERY)".to_string(),
            simvar_name: "NAV STANDBY FREQUENCY:1".to_string(),
            simvar_value: "110.50".to_string(),
        }
    }
}

type Tx = tokio::sync::mpsc::UnboundedSender<GuiCommand>;

fn section_title(ui: &mut Ui, title: &str, theme: &AeroTheme) {
    ui.label(RichText::new(title).color(theme.text_secondary).strong().small());
    ui.add_space(4.0);
}

#[allow(clippy::too_many_arguments)]
pub fn render_lvar_view(
    ui: &mut Ui,
    state: &mut LvarViewState,
    cmd_tx: &Tx,
    lvars: &[String],
    live_values: &HashMap<String, f64>,
    simvars: &[(String, Option<f64>)],
    pmdg: &[PmdgUiStatus],
    bridge_active: bool,
    theme: &AeroTheme,
) {
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        // Left: status and tools
        ui.allocate_ui_with_layout(egui::vec2(420.0, height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ScrollArea::vertical().id_salt("vars_left").auto_shrink([false, false]).show(ui, |ui| {
                if !pmdg.is_empty() {
                    render_pmdg_card(ui, cmd_tx, pmdg, theme);
                    ui.add_space(8.0);
                }
                render_bridge_card(ui, state, cmd_tx, lvars.len(), bridge_active, theme);
                ui.add_space(8.0);
                render_simvar_tools(ui, state, cmd_tx, theme);
            });
        });

        ui.add_space(8.0);

        // Right: live tables
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.search_query)
                    .hint_text("🔍 Filter SimVars and LVARs...")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(6.0);
            let q = state.search_query.to_lowercase();
            let matches = |name: &str| q.is_empty() || name.to_lowercase().contains(&q);

            let simvar_rows: Vec<(&str, Option<f64>)> =
                simvars.iter().filter(|(n, _)| matches(n)).map(|(n, v)| (n.as_str(), *v)).collect();
            let lvar_rows: Vec<(&str, Option<f64>)> =
                lvars.iter().filter(|n| matches(n)).map(|n| (n.as_str(), live_values.get(n).copied())).collect();

            let half = ((ui.available_height() - 90.0) / 2.0).max(120.0);
            value_table(ui, "simvar_table", &format!("SIMVARS  ·  {}", simvar_rows.len()), &simvar_rows, half, theme, |name, st: &mut LvarViewState| {
                st.simvar_name = name.to_string();
            }, state, "SimVars used by the active profile appear here automatically. Add more with Watch.");
            ui.add_space(8.0);
            value_table(ui, "lvar_table", &format!("LVARS  ·  {}", lvar_rows.len()), &lvar_rows, half, theme, |name, st: &mut LvarViewState| {
                st.manual_lvar_name = name.to_string();
            }, state, "LVARs appear once the SPAD.neXt WASM bridge is connected.");
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn value_table(
    ui: &mut Ui,
    id: &str,
    title: &str,
    rows: &[(&str, Option<f64>)],
    height: f32,
    theme: &AeroTheme,
    on_select: impl Fn(&str, &mut LvarViewState),
    state: &mut LvarViewState,
    empty_hint: &str,
) {
    ui_card(ui, theme, |ui| {
        ui.set_width(ui.available_width());
        section_title(ui, title, theme);
        if rows.is_empty() {
            ui.label(RichText::new(empty_hint).color(theme.text_muted).small().italics());
            return;
        }
        ScrollArea::vertical().id_salt(id).max_height(height).auto_shrink([false, true]).show_rows(ui, 22.0, rows.len(), |ui, range| {
            for (name, value) in &rows[range] {
                ui.horizontal(|ui| {
                    let resp = ui.add(egui::Label::new(RichText::new(*name).color(theme.text_primary)).sense(egui::Sense::click()));
                    if resp.on_hover_text("Click to use in the tools on the left").clicked() {
                        on_select(name, state);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (text, color) = match value {
                            Some(v) => (format_value(*v), theme.accent),
                            None => ("waiting".to_string(), theme.text_muted),
                        };
                        ui.label(RichText::new(text).color(color).monospace());
                    });
                });
            }
        });
    });
}

fn format_value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e12 {
        format!("{}", v as i64)
    } else {
        format!("{:.3}", v)
    }
}

/// Installed PMDG aircraft: SDK data broadcast switch and live status.
fn render_pmdg_card(ui: &mut Ui, cmd_tx: &Tx, rows: &[PmdgUiStatus], theme: &AeroTheme) {
    ui_card(ui, theme, |ui| {
        section_title(ui, "PMDG SDK", theme);
        for r in rows {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&r.label).color(theme.text_primary).strong());
                ui.label(RichText::new(&r.package).color(theme.text_muted).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (text, color) = if r.receiving {
                        ("receiving data", theme.success)
                    } else if r.broadcast_enabled {
                        ("waiting for aircraft", theme.warning)
                    } else {
                        ("broadcast off", theme.danger)
                    };
                    status_pill(ui, text, color);
                });
            });
        }
        if rows.iter().any(|r| !r.broadcast_enabled) {
            ui.add_space(4.0);
            ui.label(
                RichText::new("PMDG events work already. Reading cockpit data (displays, conditions) needs EnableDataBroadcast=1 in the aircraft options file.")
                    .color(theme.text_muted)
                    .small(),
            );
            ui.add_space(4.0);
            if ui.button(RichText::new("Enable data broadcast").color(theme.accent).strong()).clicked() {
                let _ = cmd_tx.send(GuiCommand::EnablePmdgBroadcast);
            }
        }
    });
}

fn render_bridge_card(ui: &mut Ui, state: &mut LvarViewState, cmd_tx: &Tx, lvar_count: usize, active: bool, theme: &AeroTheme) {
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            section_title(ui, "WASM LVAR BRIDGE", theme);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui_status_badge(ui, if active { "connected" } else { "offline" }, active, theme);
            });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} LVARs discovered", lvar_count)).color(theme.text_secondary).small());
            if ui.small_button("🔄 Rescan").clicked() {
                let _ = cmd_tx.send(GuiCommand::RequestLvarScan);
            }
        });
        ui.add_space(6.0);
        egui::Grid::new("lvar_tools").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("LVAR").color(theme.text_secondary).small());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.manual_lvar_name).desired_width(190.0));
                ui.add(egui::TextEdit::singleline(&mut state.manual_lvar_val).desired_width(60.0));
                if ui.button("Set").clicked() {
                    if let Ok(v) = state.manual_lvar_val.trim().parse::<f64>() {
                        let _ = cmd_tx.send(GuiCommand::SetLvar { name: state.manual_lvar_name.clone(), value: v });
                    }
                }
            });
            ui.end_row();
            ui.label(RichText::new("RPN").color(theme.text_secondary).small());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut state.rpn_code).desired_width(258.0).font(egui::TextStyle::Monospace));
                if ui.button("Run").clicked() {
                    let _ = cmd_tx.send(GuiCommand::ExecCalculatorCode { code: state.rpn_code.clone() });
                }
            });
            ui.end_row();
        });
    });
}

fn render_simvar_tools(ui: &mut Ui, state: &mut LvarViewState, cmd_tx: &Tx, theme: &AeroTheme) {
    ui_card(ui, theme, |ui| {
        section_title(ui, "SIMVAR", theme);
        ui.add(
            egui::TextEdit::singleline(&mut state.simvar_name)
                .hint_text("e.g. NAV STANDBY FREQUENCY:1  or  INDICATED ALTITUDE, meters")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("👁 Watch").color(theme.accent)).clicked() && !state.simvar_name.trim().is_empty() {
                let _ = cmd_tx.send(GuiCommand::WatchSimVar { name: state.simvar_name.trim().to_string() });
            }
            ui.add_space(12.0);
            ui.add(egui::TextEdit::singleline(&mut state.simvar_value).desired_width(80.0));
            if ui.button("Set value").clicked() {
                if let Ok(v) = state.simvar_value.trim().parse::<f64>() {
                    let _ = cmd_tx.send(GuiCommand::SetSimVar { name: state.simvar_name.trim().to_string(), value: v });
                }
            }
        });
    });
}

fn status_pill(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 28))
        .rounding(10.0)
        .inner_margin(egui::Vec2::new(8.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("⏺ {}", text)).color(color).small().strong());
        });
}
