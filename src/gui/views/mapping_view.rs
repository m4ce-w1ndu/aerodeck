use eframe::egui::{self, RichText, ScrollArea, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::gui::app::LiveAxisState;
use crate::gui::backend::GuiCommand;
use crate::gui::presets::{COMMON_CALCULATOR_CODES, SIMCONNECT_AXIS_PRESETS, SIMCONNECT_PRESETS};
use crate::gui::theme::AeroTheme;
use crate::gui::widgets::{ui_card, ui_clickable_card, ui_elevated_card, ui_smooth_axis_bar, ui_tag_pill};
use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::schema::{
    EventActionControl, EventActionItem, EventActionScript, EventConditionItem,
    EventConditionSimple, EventConditionsList, EventDefinition, EventDefinitionsList, EventList,
    SpadEvent, SpadProfile,
};
use crate::profile::{AircraftAssignmentManager, DiscoveredAircraft};

pub struct MappingViewState {
    pub selected_device_idx: usize,
    pub selected_input: String,
    pub filter_query: String,
    pub follow_active_input: bool,
    pub input_type_filter: InputFilterType,

    // Add action form state (collapsed by default, expands inline)
    pub show_add_action_form: bool,
    pub new_action_type: ActionTypeChoice,
    pub selected_preset_idx: usize,
    pub custom_target: String,
    pub action_value: String,
    pub selected_trigger: String,

    // Condition state
    pub use_condition: bool,
    pub cond_target: String,
    pub cond_comp: String,
    pub cond_value: String,

    // Axis calibration & options
    pub axis_invert: bool,
    pub axis_deadzone_pct: f32,
    pub selected_axis_preset_idx: usize,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum InputFilterType {
    All,
    Buttons,
    Axes,
    MappedOnly,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ActionTypeChoice {
    SimConnectPreset,
    SimConnectCustom,
    Lvar,
    CalculatorCode,
}

impl Default for MappingViewState {
    fn default() -> Self {
        Self {
            selected_device_idx: 0,
            selected_input: String::new(),
            filter_query: String::new(),
            follow_active_input: true,
            input_type_filter: InputFilterType::All,
            show_add_action_form: false,
            new_action_type: ActionTypeChoice::SimConnectPreset,
            selected_preset_idx: 0,
            custom_target: "SIMCONNECT:TOGGLE_MASTER_BATTERY".to_string(),
            action_value: "1".to_string(),
            selected_trigger: "PRESS".to_string(),
            use_condition: false,
            cond_target: "SIMCONNECT:ELECTRICAL MASTER BATTERY".to_string(),
            cond_comp: "Equals".to_string(),
            cond_value: "0".to_string(),
            axis_invert: false,
            axis_deadzone_pct: 1.0,
            selected_axis_preset_idx: 0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_mapping_view(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    _profile_name: &str,
    profile_path_input: &str,
    _active_aircraft_name: &str,
    _discovered_aircrafts: &[DiscoveredAircraft],
    _aircraft_assignments: &AircraftAssignmentManager,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    last_input: &Option<(String, String, String, Instant)>,
    discovered_lvars: &[String],
    devices: &[ConnectedDevice],
    live_axes: &HashMap<String, LiveAxisState>,
    theme: &AeroTheme,
) {
    if devices.is_empty() {
        ui_card(ui, theme, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.heading(RichText::new("No Flight Hardware Detected").color(theme.warning));
                ui.label(RichText::new("Plug in your joystick, throttle, yoke, or switch panel, then click 'Rescan Hardware'.").color(theme.text_secondary));
                ui.add_space(10.0);
                if ui.button(RichText::new("🔄 Rescan Hardware").color(theme.accent).strong()).clicked() {
                    let _ = cmd_tx.send(GuiCommand::RescanDevices);
                }
                ui.add_space(30.0);
            });
        });
        return;
    }

    if state.selected_device_idx >= devices.len() {
        state.selected_device_idx = 0;
    }

    // 1. TOP BAR: Controller Selector & Slim Live Hardware Input Monitor
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Controller:").color(theme.text_secondary).strong());

            // Controller chips
            for (idx, dev) in devices.iter().enumerate() {
                let is_selected = state.selected_device_idx == idx;
                let mapped_count = profile
                    .as_ref()
                    .and_then(|p| p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id))
                    .map(|d| d.mapped_input_count())
                    .unwrap_or(0);

                let dot_color = if mapped_count > 0 { theme.success } else { theme.text_muted };
                let chip_builder = |ui: &mut Ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("●").color(dot_color).small());
                        ui.label(
                            RichText::new(format!("{} ({} mapped)", dev.name, mapped_count))
                                .color(if is_selected { theme.accent } else { theme.text_primary })
                                .small()
                                .strong(),
                        );
                    });
                };

                let (_, resp) = ui_clickable_card(ui, theme, is_selected, chip_builder);
                if resp.clicked() {
                    state.selected_device_idx = idx;
                    state.selected_input.clear();
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Slim live hardware input monitor strip
                ui.checkbox(
                    &mut state.follow_active_input,
                    RichText::new("Auto-select active input").small().color(theme.text_secondary),
                );

                if let Some((dev_str, bound, val_str, inst)) = last_input {
                    let recent = inst.elapsed().as_millis() < 1200;
                    let dot_col = if recent { theme.success } else { theme.text_muted };
                    ui.label(
                        RichText::new(format!("Live: {} -> {} ({})", dev_str, bound, val_str))
                            .color(if recent { theme.text_primary } else { theme.text_secondary })
                            .small()
                            .strong(),
                    );
                    ui.label(RichText::new("●").color(dot_col).small());
                } else {
                    ui.label(RichText::new("Awaiting hardware input...").color(theme.text_muted).small().italics());
                }
            });
        });
    });

    // Auto-select input when hardware is physically touched
    if state.follow_active_input {
        if let Some((ref dev_name, ref bound_to, _, inst)) = last_input {
            if inst.elapsed().as_millis() < 600 {
                if let Some(active_dev) = devices.get(state.selected_device_idx) {
                    if dev_name.eq_ignore_ascii_case(&active_dev.name)
                        || dev_name.contains(&active_dev.name)
                        || active_dev.name.contains(dev_name)
                    {
                        if state.selected_input != *bound_to {
                            state.selected_input = bound_to.clone();
                        }
                    } else if let Some(matching_idx) = devices.iter().position(|d| {
                        dev_name.eq_ignore_ascii_case(&d.name)
                            || dev_name.contains(&d.name)
                            || d.name.contains(dev_name)
                    }) {
                        state.selected_device_idx = matching_idx;
                        state.selected_input = bound_to.clone();
                    }
                }
            }
        }
    }

    ui.add_space(6.0);

    // 2. MAIN TWO-COLUMN SPLIT (Left = Input List, Right = Details Panel)
    ui.columns(2, |columns| {
        // --- LEFT COLUMN: Input List ---
        render_input_list_column(
            &mut columns[0],
            state,
            profile,
            devices,
            last_input,
            live_axes,
            theme,
        );

        // --- RIGHT COLUMN: Details & Action Editor ---
        render_input_details_column(
            &mut columns[1],
            state,
            profile,
            profile_path_input,
            devices,
            discovered_lvars,
            live_axes,
            cmd_tx,
            theme,
        );
    });
}

fn render_input_list_column(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &Option<SpadProfile>,
    devices: &[ConnectedDevice],
    last_input: &Option<(String, String, String, Instant)>,
    _live_axes: &HashMap<String, LiveAxisState>,
    theme: &AeroTheme,
) {
    let active_dev = devices.get(state.selected_device_idx);
    let dev_profile = profile.as_ref().and_then(|p| {
        active_dev.and_then(|d| p.find_device_for_connected(d.vendor_id, d.product_id, &d.id))
    });

    ui_card(ui, theme, |ui| {
        // Search bar
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.filter_query)
                    .hint_text("🔍 Search inputs...")
                    .desired_width(ui.available_width() - 24.0),
            );
            if !state.filter_query.is_empty() && ui.small_button("✕").clicked() {
                state.filter_query.clear();
            }
        });

        ui.add_space(4.0);

        // Filter chips: All | Buttons | Axes | Mapped
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.input_type_filter, InputFilterType::All, "All");
            ui.selectable_value(&mut state.input_type_filter, InputFilterType::Buttons, "Buttons");
            ui.selectable_value(&mut state.input_type_filter, InputFilterType::Axes, "Axes");
            ui.selectable_value(&mut state.input_type_filter, InputFilterType::MappedOnly, "★ Mapped");
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        // Gather all inputs for the active device
        let mut all_inputs: Vec<(String, bool)> = Vec::new(); // (input_id, is_axis)
        if let Some(dev) = active_dev {
            for b in &dev.buttons {
                all_inputs.push((b.clone(), false));
            }
            for a in &dev.axes {
                all_inputs.push((a.clone(), true));
            }
        }

        // Also include any inputs defined in the profile layout that might not be in the raw hardware report
        if let Some(dp) = dev_profile {
            for page in &dp.pages {
                if let Some(ref evs) = page.events {
                    for ev in &evs.items {
                        if !all_inputs.iter().any(|(id, _)| id.eq_ignore_ascii_case(&ev.bound_to)) {
                            let is_ax = ev.bound_to.contains("AXIS") || ev.bound_to.contains("THROTTLE") || ev.bound_to.contains("RUDDER");
                            all_inputs.push((ev.bound_to.clone(), is_ax));
                        }
                    }
                }
            }
        }

        let query = state.filter_query.trim().to_lowercase();
        let filtered: Vec<(String, bool, usize, Option<String>)> = all_inputs
            .into_iter()
            .filter_map(|(id, is_axis)| {
                let mapped_info = dev_profile.and_then(|dp| {
                    dp.pages.iter().find_map(|p| {
                        p.events.as_ref().and_then(|evs| {
                            evs.items.iter().find(|e| crate::profile::schema::bound_to_matches(&e.bound_to, &id))
                        })
                    })
                });

                let action_count = mapped_info
                    .and_then(|e| e.event_definitions.as_ref())
                    .map(|d| d.items.iter().map(|def| def.actions.as_ref().map(|a| a.items.len()).unwrap_or(0)).sum())
                    .unwrap_or(0);

                let summary = mapped_info.and_then(|e| {
                    e.event_definitions.as_ref().and_then(|d| {
                        d.items.first().and_then(|def| {
                            def.actions.as_ref().and_then(|a| {
                                a.items.first().map(|act| match act {
                                    EventActionItem::Control(c) => {
                                        c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string()
                                    }
                                    EventActionItem::ChangeValue(cv) => format!("LVAR:{}", cv.target.strip_prefix("LVAR:").unwrap_or(&cv.target)),
                                    EventActionItem::Script(_) => "RPN Script".to_string(),
                                    EventActionItem::CustomAxis(ca) => ca.target.clone(),
                                    _ => "Action".to_string(),
                                })
                            })
                        })
                    })
                });

                // Apply type filter
                match state.input_type_filter {
                    InputFilterType::All => {}
                    InputFilterType::Buttons => if is_axis { return None; }
                    InputFilterType::Axes => if !is_axis { return None; }
                    InputFilterType::MappedOnly => if action_count == 0 { return None; }
                }

                // Apply query filter
                if !query.is_empty() {
                    let match_id = id.to_lowercase().contains(&query);
                    let match_summary = summary.as_ref().map(|s| s.to_lowercase().contains(&query)).unwrap_or(false);
                    if !match_id && !match_summary {
                        return None;
                    }
                }

                Some((id, is_axis, action_count, summary))
            })
            .collect();

        // Virtualized scroll list
        let row_height = 34.0;
        let total_rows = filtered.len();

        ScrollArea::vertical()
            .id_salt("mapping_inputs_scroll")
            .max_height(ui.available_height() - 10.0)
            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                for idx in row_range {
                    if let Some((ref input_id, is_axis, action_count, ref summary)) = filtered.get(idx) {
                        let is_selected = state.selected_input == *input_id;

                        // Check if live pressed
                        let is_live_active = if let Some((ref dev_str, ref bound, _, inst)) = last_input {
                            inst.elapsed().as_millis() < 800
                                && bound.eq_ignore_ascii_case(input_id)
                                && active_dev.map(|d| dev_str.contains(&d.name) || d.name.contains(dev_str)).unwrap_or(false)
                        } else {
                            false
                        };

                        let row_builder = |ui: &mut Ui| {
                            ui.horizontal(|ui| {
                                // Mapped indicator
                                let (dot_icon, dot_col) = if is_live_active {
                                    ("●", theme.accent)
                                } else if *action_count > 0 {
                                    ("●", theme.success)
                                } else {
                                    ("○", theme.text_muted)
                                };
                                ui.label(RichText::new(dot_icon).color(dot_col).small().strong());

                                // Input name
                                let label_text = if *is_axis {
                                    format!("⚡ {}", input_id)
                                } else {
                                    (*input_id).clone()
                                };
                                ui.label(
                                    RichText::new(label_text)
                                        .color(if is_selected { theme.accent } else { theme.text_primary })
                                        .strong(),
                                );

                                // Short binding summary
                                if let Some(ref sum) = summary {
                                    let display_sum = if *action_count > 1 {
                                        format!("[{} +{}]", sum, action_count - 1)
                                    } else {
                                        format!("[{}]", sum)
                                    };
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(display_sum).color(theme.text_secondary).small());
                                    });
                                }
                            });
                        };

                        let (_, resp) = ui_clickable_card(ui, theme, is_selected, row_builder);
                        if resp.clicked() {
                            state.selected_input = (*input_id).clone();
                            state.show_add_action_form = false;
                        }

                        ui.add_space(2.0);
                    }
                }
            });
    });
}

#[allow(clippy::too_many_arguments)]
fn render_input_details_column(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    devices: &[ConnectedDevice],
    discovered_lvars: &[String],
    live_axes: &HashMap<String, LiveAxisState>,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    let active_dev = devices.get(state.selected_device_idx);

    // If no input is selected, render helpful empty state
    if state.selected_input.is_empty() {
        ui_card(ui, theme, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.heading(RichText::new("🎮 No Input Selected").color(theme.text_primary).strong());
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Select a button, switch, or axis from the list on the left to configure or test its actions.")
                        .color(theme.text_secondary),
                );
                ui.label(
                    RichText::new("Or move a control on your physical flight hardware to highlight it automatically.")
                        .color(theme.text_muted)
                        .small(),
                );
                ui.add_space(20.0);

                if let Some(dev) = active_dev {
                    ui_elevated_card(ui, theme, |ui| {
                        ui.label(RichText::new(format!("Current Controller: {}", dev.name)).color(theme.accent).strong());
                        ui.label(RichText::new(format!("VID:PID: 0x{:04X}:0x{:04X}  •  {} buttons  •  {} axes", dev.vendor_id, dev.product_id, dev.buttons.len(), dev.axes.len())).color(theme.text_secondary).small());
                    });
                }
                ui.add_space(40.0);
            });
        });
        return;
    }

    let is_axis = state.selected_input.contains("AXIS")
        || state.selected_input.contains("THROTTLE")
        || state.selected_input.contains("RUDDER");

    // Header Card for Selected Input
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new(format!("Input: {}", state.selected_input))
                    .color(theme.text_primary)
                    .strong(),
            );
            if let Some(dev) = active_dev {
                ui_tag_pill(ui, &dev.name, theme.accent, theme);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("✕ Deselect").color(theme.text_muted).small()).clicked() {
                    state.selected_input.clear();
                }
            });
        });
    });

    ui.add_space(6.0);

    // If input is an Axis: Live Axis Telemetry & Calibration
    if is_axis {
        render_axis_telemetry_card(ui, state, profile, profile_path_input, active_dev, live_axes, cmd_tx, theme);
        ui.add_space(6.0);
    }

    // Configured Actions Section
    render_configured_actions_section(
        ui,
        state,
        profile,
        profile_path_input,
        active_dev,
        discovered_lvars,
        cmd_tx,
        theme,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_axis_telemetry_card(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    active_dev: Option<&ConnectedDevice>,
    live_axes: &HashMap<String, LiveAxisState>,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("AXIS TELEMETRY & CALIBRATION").color(theme.text_secondary).strong().small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("Live analog gauge").color(theme.text_muted).small());
            });
        });

        ui.add_space(4.0);

        let live_val = live_axes
            .get(&state.selected_input)
            .or_else(|| {
                active_dev.and_then(|d| live_axes.get(&format!("{}:{}", d.name, state.selected_input)))
            });

        let (norm, raw) = live_val
            .map(|st| (st.normalized, st.raw))
            .unwrap_or((0.0, 0));

        let is_bipolar = !state.selected_input.contains("THROTTLE");
        let effective_norm = if state.axis_invert { -norm } else { norm };

        ui.horizontal(|ui| {
            ui_smooth_axis_bar(ui, effective_norm, is_bipolar, theme);
            ui.label(
                RichText::new(format!("{:+.1}%  (raw: {})", effective_norm * 100.0, raw))
                    .color(theme.text_primary)
                    .monospace()
                    .strong(),
            );
        });

        ui.add_space(6.0);

        // Calibration options
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.axis_invert, RichText::new("Invert Axis").color(theme.text_primary).small());
            ui.add_space(10.0);
            ui.label(RichText::new("Deadzone:").color(theme.text_secondary).small());
            ui.add(egui::Slider::new(&mut state.axis_deadzone_pct, 0.0..=15.0).suffix("%"));

            ui.add_space(10.0);
            ui.label(RichText::new("Preset:").color(theme.text_secondary).small());
            egui::ComboBox::from_id_salt("axis_preset_combo")
                .width(180.0)
                .selected_text(SIMCONNECT_AXIS_PRESETS[state.selected_axis_preset_idx].name)
                .show_ui(ui, |ui| {
                    for (idx, p) in SIMCONNECT_AXIS_PRESETS.iter().enumerate() {
                        ui.selectable_value(&mut state.selected_axis_preset_idx, idx, p.name);
                    }
                });

            if ui.button(RichText::new("✔ Apply Axis").color(theme.success).strong().small()).clicked() {
                if let Some(conn_dev) = active_dev {
                    let preset = &SIMCONNECT_AXIS_PRESETS[state.selected_axis_preset_idx];
                    let event_name = preset.name;
                    add_axis_action_to_profile(profile, conn_dev, &state.selected_input, event_name);
                    save_profile_now(profile, profile_path_input, cmd_tx);
                }
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn render_configured_actions_section(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    active_dev: Option<&ConnectedDevice>,
    discovered_lvars: &[String],
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("CONFIGURED ACTIONS").color(theme.text_secondary).strong().small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !state.show_add_action_form && ui.button(RichText::new("➕ Add Action").color(theme.accent).strong().small()).clicked() {
                    state.show_add_action_form = true;
                }
            });
        });

        ui.add_space(4.0);

        let mut to_remove: Option<(usize, usize)> = None;
        let mut to_test: Option<GuiCommand> = None;
        let mut action_cards_count = 0;

        if let (Some(prof), Some(conn_dev)) = (profile.as_ref(), active_dev) {
            if let Some(dev_prof) = prof.find_device_for_connected(conn_dev.vendor_id, conn_dev.product_id, &conn_dev.id) {
                for page in &dev_prof.pages {
                    if let Some(ref evs) = page.events {
                        for ev in &evs.items {
                            if crate::profile::schema::bound_to_matches(&ev.bound_to, &state.selected_input) {
                                if let Some(ref defs) = ev.event_definitions {
                                    for (def_idx, def) in defs.items.iter().enumerate() {
                                        let trigger = def.trigger.as_deref().unwrap_or("PRESS");
                                        let cond_summary = def.conditions.as_ref().and_then(|c| {
                                            c.items.first().map(|ci| match ci {
                                                EventConditionItem::Simple(s) => format!("When {} {} {:?}", s.target, s.comparator.as_deref().unwrap_or("=="), s.value),
                                                EventConditionItem::Expression(e) => format!("When {}", e.expression),
                                            })
                                        });

                                        if let Some(ref acts) = def.actions {
                                            for (act_idx, act) in acts.items.iter().enumerate() {
                                                action_cards_count += 1;
                                                ui_elevated_card(ui, theme, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui_tag_pill(ui, trigger, theme.accent, theme);

                                                        match act {
                                                            EventActionItem::Control(c) => {
                                                                ui.label(RichText::new(format!("⚙ SimConnect: {}", c.target)).color(theme.text_primary).strong());
                                                            }
                                                            EventActionItem::ChangeValue(cv) => {
                                                                ui.label(RichText::new(format!("📝 LVAR: {} = {:?}", cv.target, cv.value)).color(theme.text_primary).strong());
                                                            }
                                                            EventActionItem::Script(sc) => {
                                                                ui.label(RichText::new(format!("💻 RPN: {}", sc.script.as_deref().unwrap_or_default())).color(theme.text_primary).strong());
                                                            }
                                                            EventActionItem::CustomAxis(ca) => {
                                                                ui.label(RichText::new(format!("⚡ Custom Axis: {}", ca.target)).color(theme.text_primary).strong());
                                                            }
                                                            _ => {
                                                                ui.label(RichText::new("Action").color(theme.text_primary));
                                                            }
                                                        }

                                                        if let Some(ref cs) = cond_summary {
                                                            ui_tag_pill(ui, cs, theme.warning, theme);
                                                        }

                                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                            if ui.button(RichText::new("🗑 Delete").color(theme.danger).small()).clicked() {
                                                                to_remove = Some((def_idx, act_idx));
                                                            }

                                                            // Test in simulator button
                                                            if ui.button(RichText::new("⚡ Test in Sim").color(theme.accent).strong().small()).clicked() {
                                                                match act {
                                                                    EventActionItem::Control(c) => {
                                                                        let val = c.value.as_deref().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                                                                        let name = c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string();
                                                                        to_test = Some(GuiCommand::FireSimConnectEvent { event_name: name, value: val });
                                                                    }
                                                                    EventActionItem::ChangeValue(cv) => {
                                                                        let val = cv.value.as_deref().and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0);
                                                                        let name = cv.target.strip_prefix("LVAR:").unwrap_or(&cv.target).to_string();
                                                                        to_test = Some(GuiCommand::SetLvar { name, value: val });
                                                                    }
                                                                    EventActionItem::Script(sc) => {
                                                                        let code = sc.script.clone().unwrap_or_default();
                                                                        to_test = Some(GuiCommand::ExecCalculatorCode { code });
                                                                    }
                                                                    _ => {}
                                                                }
                                                            }
                                                        });
                                                    });
                                                });
                                                ui.add_space(2.0);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if action_cards_count == 0 {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("No actions configured for this input yet.").color(theme.text_muted).italics());
                ui.add_space(4.0);
            });
        }

        if let Some(cmd) = to_test {
            let _ = cmd_tx.send(cmd);
        }

        if let (Some((def_idx, act_idx)), Some(conn_dev)) = (to_remove, active_dev) {
            remove_action_from_device(profile, conn_dev, &state.selected_input, def_idx, act_idx);
            save_profile_now(profile, profile_path_input, cmd_tx);
        }

        // Inline Add Action Form (expands when "+ Add action" is clicked)
        if state.show_add_action_form {
            ui.add_space(8.0);
            render_inline_add_action_form(
                ui,
                state,
                profile,
                profile_path_input,
                active_dev,
                discovered_lvars,
                cmd_tx,
                theme,
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn render_inline_add_action_form(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    active_dev: Option<&ConnectedDevice>,
    discovered_lvars: &[String],
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    egui::Frame::none()
        .fill(theme.bg_surface)
        .stroke(egui::Stroke::new(1.0_f32, theme.accent))
        .rounding(6.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("➕ Configure New Action").color(theme.accent).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕ Close").clicked() {
                        state.show_add_action_form = false;
                    }
                });
            });

            ui.add_space(4.0);

            // Trigger selector
            ui.horizontal(|ui| {
                ui.label(RichText::new("Event Trigger:").color(theme.text_secondary).small());
                ui.selectable_value(&mut state.selected_trigger, "PRESS".to_string(), "On Press");
                ui.selectable_value(&mut state.selected_trigger, "RELEASE".to_string(), "On Release");
                ui.selectable_value(&mut state.selected_trigger, "HELD".to_string(), "While Held");
                ui.selectable_value(&mut state.selected_trigger, "CW".to_string(), "Clockwise");
                ui.selectable_value(&mut state.selected_trigger, "CCW".to_string(), "Counter-Clockwise");
            });

            ui.add_space(4.0);

            // Action type choices
            ui.horizontal(|ui| {
                ui.label(RichText::new("Action Type:").color(theme.text_secondary).small());
                ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::SimConnectPreset, "SimConnect Preset");
                ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::SimConnectCustom, "Custom SimEvent");
                ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::Lvar, "MSFS LVAR");
                ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::CalculatorCode, "RPN Calculator");
            });

            ui.add_space(6.0);

            // Specific action parameters
            match state.new_action_type {
                ActionTypeChoice::SimConnectPreset => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Preset:").color(theme.text_secondary).small());
                        let current_preset = &SIMCONNECT_PRESETS[state.selected_preset_idx];
                        egui::ComboBox::from_id_salt("mapping_preset_combo")
                            .width(340.0)
                            .selected_text(format!("[{}] {}", current_preset.category, current_preset.name))
                            .show_ui(ui, |ui| {
                                for (idx, p) in SIMCONNECT_PRESETS.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut state.selected_preset_idx,
                                        idx,
                                        format!("[{}] {} - {}", p.category, p.name, p.description),
                                    );
                                }
                            });
                    });
                    ui.label(
                        RichText::new(SIMCONNECT_PRESETS[state.selected_preset_idx].description)
                            .italics()
                            .color(theme.text_secondary)
                            .small(),
                    );
                }
                ActionTypeChoice::SimConnectCustom => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Event Name:").color(theme.text_secondary).small());
                        ui.add(egui::TextEdit::singleline(&mut state.custom_target).hint_text("e.g. AP_MASTER or TOGGLE_MASTER_BATTERY"));
                        ui.label(RichText::new("Value:").color(theme.text_secondary).small());
                        ui.add(egui::TextEdit::singleline(&mut state.action_value).desired_width(60.0));
                    });
                }
                ActionTypeChoice::Lvar => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Target LVAR:").color(theme.text_secondary).small());
                        ui.add(egui::TextEdit::singleline(&mut state.custom_target).hint_text("LVAR:A32NX_AUTOPILOT_1_ACTIVE"));
                        ui.label(RichText::new("Value:").color(theme.text_secondary).small());
                        ui.add(egui::TextEdit::singleline(&mut state.action_value).desired_width(60.0));
                    });
                    if !discovered_lvars.is_empty() {
                        egui::ComboBox::from_id_salt("discovered_lvar_quick_combo")
                            .selected_text(RichText::new("Pick from discovered LVARs...").color(theme.text_secondary).small())
                            .width(300.0)
                            .show_ui(ui, |ui| {
                                for lvar in discovered_lvars.iter().take(50) {
                                    if ui.selectable_label(false, lvar).clicked() {
                                        state.custom_target = format!("LVAR:{}", lvar);
                                    }
                                }
                            });
                    }
                }
                ActionTypeChoice::CalculatorCode => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("RPN Code:").color(theme.text_secondary).small());
                        ui.add(egui::TextEdit::singleline(&mut state.custom_target).hint_text("e.g. (L:XMLVAR_Baro1_Mode, Number) 1 == if{ ... }").desired_width(320.0));
                    });
                    egui::ComboBox::from_id_salt("calculator_templates_combo")
                        .selected_text(RichText::new("Templates...").color(theme.text_secondary).small())
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            for (label, code) in COMMON_CALCULATOR_CODES {
                                if ui.selectable_label(false, *label).clicked() {
                                    state.custom_target = (*code).to_string();
                                }
                            }
                        });
                }
            }

            ui.add_space(6.0);

            // Optional Execution Condition
            ui.checkbox(&mut state.use_condition, RichText::new("Add Execution Condition").color(theme.text_primary).small());
            if state.use_condition {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("When:").color(theme.text_secondary).small());
                    ui.add(egui::TextEdit::singleline(&mut state.cond_target).hint_text("LVAR or SimVar").desired_width(180.0));
                    egui::ComboBox::from_id_salt("cond_comp_combo")
                        .width(90.0)
                        .selected_text(&state.cond_comp)
                        .show_ui(ui, |ui| {
                            for comp in ["Equals", "Unequal", "Less", "Greater"] {
                                ui.selectable_value(&mut state.cond_comp, comp.to_string(), comp);
                            }
                        });
                    ui.add(egui::TextEdit::singleline(&mut state.cond_value).desired_width(60.0));
                });
            }

            ui.add_space(8.0);

            // Buttons: Save Action & Test Action
            ui.horizontal(|ui| {
                if ui.button(RichText::new("✔ Save Action to Profile").color(theme.success).strong()).clicked() {
                    if let Some(conn_dev) = active_dev {
                        save_action_to_profile(state, profile, conn_dev);
                        save_profile_now(profile, profile_path_input, cmd_tx);
                        state.show_add_action_form = false;
                    }
                }

                if ui.button(RichText::new("⚡ Test Action in Simulator").color(theme.accent).strong()).clicked() {
                    test_new_action_in_sim(state, cmd_tx);
                }

                if ui.button("Cancel").clicked() {
                    state.show_add_action_form = false;
                }
            });
        });
}

fn save_action_to_profile(
    state: &MappingViewState,
    profile: &mut Option<SpadProfile>,
    conn_dev: &ConnectedDevice,
) {
    let Some(ref mut prof) = profile else { return };
    let dev_prof = prof.ensure_device_for_connected(
        conn_dev.vendor_id,
        conn_dev.product_id,
        &conn_dev.id,
        &conn_dev.name,
        None,
    );

    if dev_prof.pages.is_empty() {
        dev_prof.pages.push(crate::profile::schema::DevicePage {
            id: Some("Default".to_string()),
            is_default: Some(true),
            name: Some("Default".to_string()),
            events: Some(EventList { items: Vec::new() }),
        });
    }

    let events = dev_prof.pages[0].events.get_or_insert_with(|| EventList { items: Vec::new() });
    let spad_event = if let Some(existing) = events.items.iter_mut().find(|e| crate::profile::schema::bound_to_matches(&e.bound_to, &state.selected_input)) {
        existing
    } else {
        events.items.push(SpadEvent {
            bound_to: state.selected_input.clone(),
            config_id: None,
            event_definitions: Some(EventDefinitionsList { items: Vec::new() }),
        });
        events.items.last_mut().unwrap()
    };

    let defs = spad_event.event_definitions.get_or_insert_with(|| EventDefinitionsList { items: Vec::new() });

    let action_item = match state.new_action_type {
        ActionTypeChoice::SimConnectPreset => {
            let p = &SIMCONNECT_PRESETS[state.selected_preset_idx];
            EventActionItem::Control(EventActionControl {
                target: format!("SIMCONNECT:{}", p.name),
                value: Some("1".to_string()),
            })
        }
        ActionTypeChoice::SimConnectCustom => {
            let target = if state.custom_target.starts_with("SIMCONNECT:") {
                state.custom_target.clone()
            } else {
                format!("SIMCONNECT:{}", state.custom_target.trim())
            };
            EventActionItem::Control(EventActionControl {
                target,
                value: Some(state.action_value.clone()),
            })
        }
        ActionTypeChoice::Lvar => {
            let target = if state.custom_target.starts_with("LVAR:") {
                state.custom_target.clone()
            } else {
                format!("LVAR:{}", state.custom_target.trim())
            };
            EventActionItem::ChangeValue(crate::profile::schema::EventActionChangeValue {
                target,
                operation: Some("Set".to_string()),
                value: Some(state.action_value.clone()),
            })
        }
        ActionTypeChoice::CalculatorCode => {
            EventActionItem::Script(EventActionScript {
                script: Some(state.custom_target.clone()),
                language: Some("RPN".to_string()),
            })
        }
    };

    let conditions = if state.use_condition {
        Some(EventConditionsList {
            items: vec![EventConditionItem::Simple(EventConditionSimple {
                target: state.cond_target.clone(),
                comparator: Some(state.cond_comp.clone()),
                value: Some(state.cond_value.clone()),
            })],
        })
    } else {
        None
    };

    defs.items.push(EventDefinition {
        trigger: Some(state.selected_trigger.clone()),
        priority: None,
        is_final: Some(true),
        condition_binding: None,
        conditions,
        actions: Some(crate::profile::schema::EventActionsList { items: vec![action_item] }),
    });
}

fn add_axis_action_to_profile(
    profile: &mut Option<SpadProfile>,
    conn_dev: &ConnectedDevice,
    bound_to: &str,
    event_name: &str,
) {
    let Some(ref mut prof) = profile else { return };
    let dev_prof = prof.ensure_device_for_connected(
        conn_dev.vendor_id,
        conn_dev.product_id,
        &conn_dev.id,
        &conn_dev.name,
        None,
    );

    if dev_prof.pages.is_empty() {
        dev_prof.pages.push(crate::profile::schema::DevicePage {
            id: Some("Default".to_string()),
            is_default: Some(true),
            name: Some("Default".to_string()),
            events: Some(EventList { items: Vec::new() }),
        });
    }

    let events = dev_prof.pages[0].events.get_or_insert_with(|| EventList { items: Vec::new() });
    let spad_event = if let Some(existing) = events.items.iter_mut().find(|e| crate::profile::schema::bound_to_matches(&e.bound_to, bound_to)) {
        existing
    } else {
        events.items.push(SpadEvent {
            bound_to: bound_to.to_string(),
            config_id: None,
            event_definitions: Some(EventDefinitionsList { items: Vec::new() }),
        });
        events.items.last_mut().unwrap()
    };

    let defs = spad_event.event_definitions.get_or_insert_with(|| EventDefinitionsList { items: Vec::new() });
    defs.items.clear();
    defs.items.push(EventDefinition {
        trigger: Some("VALUE".to_string()),
        priority: None,
        is_final: Some(true),
        condition_binding: None,
        conditions: None,
        actions: Some(crate::profile::schema::EventActionsList {
            items: vec![EventActionItem::Control(EventActionControl {
                target: format!("SIMCONNECT:{}", event_name),
                value: None,
            })],
        }),
    });
}

fn remove_action_from_device(
    profile: &mut Option<SpadProfile>,
    conn_dev: &ConnectedDevice,
    input: &str,
    def_idx: usize,
    act_idx: usize,
) {
    let Some(ref mut prof) = profile else { return };
    if let Some(dev_prof) = prof.devices.as_mut().and_then(|d| {
        d.items.iter_mut().find(|dev| dev.matches_vid_pid(conn_dev.vendor_id, conn_dev.product_id))
    }) {
        for page in &mut dev_prof.pages {
            if let Some(ref mut evs) = page.events {
                for ev in &mut evs.items {
                    if crate::profile::schema::bound_to_matches(&ev.bound_to, input) {
                        if let Some(ref mut defs) = ev.event_definitions {
                            if let Some(def) = defs.items.get_mut(def_idx) {
                                if let Some(ref mut acts) = def.actions {
                                    if act_idx < acts.items.len() {
                                        acts.items.remove(act_idx);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn test_new_action_in_sim(
    state: &MappingViewState,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    match state.new_action_type {
        ActionTypeChoice::SimConnectPreset => {
            let p = &SIMCONNECT_PRESETS[state.selected_preset_idx];
            let name = p.name;
            let _ = cmd_tx.send(GuiCommand::FireSimConnectEvent {
                event_name: name.to_string(),
                value: 1,
            });
        }
        ActionTypeChoice::SimConnectCustom => {
            let name = state.custom_target.strip_prefix("SIMCONNECT:").unwrap_or(&state.custom_target);
            let val = state.action_value.parse::<u32>().unwrap_or(1);
            let _ = cmd_tx.send(GuiCommand::FireSimConnectEvent {
                event_name: name.to_string(),
                value: val,
            });
        }
        ActionTypeChoice::Lvar => {
            let name = state.custom_target.strip_prefix("LVAR:").unwrap_or(&state.custom_target);
            let val = state.action_value.parse::<f64>().unwrap_or(1.0);
            let _ = cmd_tx.send(GuiCommand::SetLvar {
                name: name.to_string(),
                value: val,
            });
        }
        ActionTypeChoice::CalculatorCode => {
            let _ = cmd_tx.send(GuiCommand::ExecCalculatorCode {
                code: state.custom_target.clone(),
            });
        }
    }
}

fn save_profile_now(
    profile: &Option<SpadProfile>,
    path_str: &str,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    if let Some(ref prof) = profile {
        let p = PathBuf::from(path_str);
        let _ = cmd_tx.send(GuiCommand::UpdateAndSaveProfile {
            profile: prof.clone(),
            path: p,
            bind_to_aircraft: None,
        });
    }
}
