use eframe::egui::{self, RichText, ScrollArea, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::gui::app::LiveAxisState;
use crate::gui::backend::GuiCommand;
use crate::gui::presets::{COMMON_CALCULATOR_CODES, SIMCONNECT_AXIS_PRESETS};
use crate::gui::theme::AeroTheme;
use crate::gui::widgets::{
    ui_card, ui_clickable_card, ui_clickable_row_card, ui_elevated_card, ui_smooth_axis_bar,
    ui_tag_pill,
};
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

    pub axis_mode: AxisMode,
    pub lever_draft: Option<crate::gui::views::lever_editor::LeverDraft>,
    /// Hat direction picked in the add-action form (`NORTH`, ...)
    pub pov_dir: String,
    /// Trigger kind picked in the add-action form (`PRESS`, `PRESSLONG`, ...)
    pub trigger_kind: String,

    // PMDG event picker
    pub pmdg_family_idx: usize,
    pub pmdg_query: String,
    pub pmdg_event: String,
    pub pmdg_param_idx: usize,
    /// Category filter in the SimConnect event picker (None = all)
    pub sim_event_category: Option<&'static str>,
}

/// PMDG event parameters: (label, value). Values are PMDG mouse flags.
const PMDG_PARAMS: [(&str, u32); 4] = [
    ("Click", crate::pmdg::MOUSE_FLAG_LEFTSINGLE),
    ("Wheel up (knob clockwise)", crate::pmdg::MOUSE_FLAG_WHEEL_UP),
    ("Wheel down (knob counter-clockwise)", crate::pmdg::MOUSE_FLAG_WHEEL_DOWN),
    ("Release", crate::pmdg::MOUSE_FLAG_LEFTRELEASE),
];

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AxisMode {
    /// One sim axis event (ailerons, elevator, ...)
    Simple,
    /// Calibrated lever with full / idle zones (throttle quadrants)
    Lever,
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
    SimConnectCustom,
    Pmdg,
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
            new_action_type: ActionTypeChoice::SimConnectCustom,
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
            axis_mode: AxisMode::Simple,
            lever_draft: None,
            pov_dir: "NORTH".to_string(),
            trigger_kind: "PRESS".to_string(),
            pmdg_family_idx: 0,
            pmdg_query: String::new(),
            pmdg_event: String::new(),
            pmdg_param_idx: 0,
            sim_event_category: None,
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
                        ui.label(RichText::new("⏺").color(dot_color).small());
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
                    ui.label(RichText::new("⏺").color(dot_col).small());
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
    let avail_height = ui.available_height();
    ui.horizontal_top(|ui| {
        // --- LEFT COLUMN: Input List ---
        ui.allocate_ui_with_layout(
            egui::vec2(340.0, avail_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                render_input_list_column(
                    ui,
                    state,
                    profile,
                    devices,
                    last_input,
                    live_axes,
                    theme,
                );
            },
        );

        ui.separator();

        // --- RIGHT COLUMN: Details & Action Editor ---
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), avail_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ScrollArea::vertical()
                    .id_salt("mapping_details_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_input_details_column(
                            ui,
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
            },
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
            if !state.filter_query.is_empty() && ui.small_button("✖").clicked() {
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
                            let is_ax = is_axis_input_name(&ev.bound_to);
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
                                    EventActionItem::CustomAxis(ca) => ca.target.strip_prefix("SIMCONNECT:").unwrap_or(&ca.target).to_string(),
                                    EventActionItem::RangedAxis(ra) => format!("Lever {}", ra.config_id.trim_start_matches("LEVER_").replace("NOREVERSE_", "").to_lowercase()),
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
            .auto_shrink([false, false])
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
                                    ("⏺", theme.accent)
                                } else if *action_count > 0 {
                                    ("⏺", theme.success)
                                } else {
                                    ("○", theme.text_muted)
                                };
                                ui.label(RichText::new(dot_icon).color(dot_col).small().strong());

                                // Input name
                                let icon = if *is_axis { "⚡" } else if input_id.starts_with("HATSWITCH") { "🕹" } else { " " };
                                ui.label(RichText::new(icon).color(theme.text_muted).small());
                                ui.label(
                                    RichText::new(input_label(input_id))
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

                        let (_, resp) = ui_clickable_row_card(ui, theme, is_selected, row_builder);
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

    let is_axis = active_dev
        .map(|d| d.axes.iter().any(|a| a.eq_ignore_ascii_case(&state.selected_input)))
        .unwrap_or(false)
        || is_axis_input_name(&state.selected_input);

    // Header Card for Selected Input
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new(input_label(&state.selected_input))
                    .color(theme.text_primary)
                    .strong(),
            );
            if let Some(dev) = active_dev {
                ui_tag_pill(ui, &dev.name, theme.accent, theme);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("✖ Deselect").color(theme.text_muted).small()).clicked() {
                    state.selected_input.clear();
                }
            });
        });
    });

    ui.add_space(6.0);

    // If input is an Axis: Live Axis Telemetry & Calibration
    if is_axis {
        let existing = active_dev.and_then(|d| {
            crate::gui::views::lever_editor::existing_lever(profile, d.vendor_id, d.product_id, &d.id, &state.selected_input)
        });
        // Default to the lever editor for quadrants and inputs that already have a lever.
        if state.lever_draft.as_ref().map(|l| l.input != state.selected_input).unwrap_or(true) {
            let quadrant = active_dev.is_some_and(|d| d.category == crate::hardware::enumerator::DeviceCategory::ThrottleQuadrant);
            state.axis_mode = if existing.is_some() || quadrant { AxisMode::Lever } else { AxisMode::Simple };
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.axis_mode, AxisMode::Simple, "Axis to sim event");
            ui.selectable_value(&mut state.axis_mode, AxisMode::Lever, "Calibrated lever (throttle, mixture, flaps)");
        });
        ui.add_space(4.0);
        match state.axis_mode {
            AxisMode::Simple => {
                // Keep the draft keyed to this input so switching modes does not reset the choice.
                if state.lever_draft.as_ref().map(|l| l.input != state.selected_input).unwrap_or(true) {
                    state.lever_draft = Some(crate::gui::views::lever_editor::LeverDraft {
                        input: state.selected_input.clone(),
                        action: existing.clone().unwrap_or(crate::profile::schema::EventActionRangedAxis {
                            config_id: crate::gui::views::lever_editor::LEVER_KINDS[0].0.to_string(),
                            options: None,
                        }),
                    });
                }
                render_axis_telemetry_card(ui, state, profile, profile_path_input, active_dev, live_axes, cmd_tx, theme);
            }
            AxisMode::Lever => {
                let live = live_axes
                    .get(&state.selected_input)
                    .or_else(|| active_dev.and_then(|d| live_axes.get(&format!("{}:{}", d.name, state.selected_input))));
                let input = state.selected_input.clone();
                if let Some(action) = crate::gui::views::lever_editor::render_lever_card(ui, &mut state.lever_draft, &input, existing, live, theme) {
                    if let Some(conn_dev) = active_dev {
                        set_value_definition(profile, conn_dev, &input, vec![EventActionItem::RangedAxis(action)]);
                        save_profile_now(profile, profile_path_input, cmd_tx);
                    }
                }
            }
        }
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

        // Normalized values are -1..1 for devices with a signed range, 0..1 otherwise.
        let is_bipolar = live_val
            .map(|st| st.min < 0)
            .unwrap_or(!state.selected_input.contains("THROTTLE"));
        let effective_norm = match (state.axis_invert, is_bipolar) {
            (false, _) => norm,
            (true, true) => -norm,
            (true, false) => 1.0 - norm,
        };

        ui_smooth_axis_bar(ui, effective_norm, is_bipolar, theme);
        ui.label(
            RichText::new(format!("{:+.1}%   raw {}", effective_norm * 100.0, raw))
                .color(theme.text_primary)
                .monospace()
                .strong(),
        );

        ui.add_space(6.0);

        // Calibration options
        ui.horizontal_wrapped(|ui| {
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
                    add_axis_action_to_profile(profile, conn_dev, &state.selected_input, event_name, state.axis_invert);
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
                                                        ui_tag_pill(ui, &trigger_label(trigger), theme.accent, theme);

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
                                                            EventActionItem::RangedAxis(ra) => {
                                                                let engines: Vec<String> = (1..=4).filter(|n| ra.option_bool(&format!("OPTION{}", n))).map(|n| n.to_string()).collect();
                                                                let kind = crate::gui::views::lever_editor::LEVER_KINDS
                                                                    .iter()
                                                                    .find(|(id, _)| id.eq_ignore_ascii_case(&ra.config_id))
                                                                    .map(|(_, l)| *l)
                                                                    .unwrap_or(&ra.config_id);
                                                                let eng = if engines.is_empty() { "all engines".to_string() } else { format!("engine {}", engines.join(" + ")) };
                                                                ui.label(RichText::new(format!("🕹 Lever: {} ({})", kind, eng)).color(theme.text_primary).strong());
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
                                                                        let param = c.effective_value().map(str::trim).filter(|v| !v.is_empty());
                                                                        if let Some((prefix, name)) = crate::profile::executor::pmdg_target(&c.target) {
                                                                            let parameter = param
                                                                                .and_then(|v| v.parse::<i64>().ok())
                                                                                .map(|v| v as u32)
                                                                                .unwrap_or_else(|| crate::pmdg::default_parameter(trigger));
                                                                            to_test = Some(GuiCommand::FirePmdgEvent { prefix: prefix.to_string(), name: name.to_string(), parameter });
                                                                        } else {
                                                                            let val = param.and_then(|v| v.parse::<i64>().ok()).map(|v| v as u32).unwrap_or(1);
                                                                            let name = c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string();
                                                                            to_test = Some(GuiCommand::FireSimConnectEvent { event_name: name, value: val });
                                                                        }
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
                    if ui.small_button("✖ Close").clicked() {
                        state.show_add_action_form = false;
                    }
                });
            });

            ui.add_space(4.0);

            // Trigger selector
            let is_hat = state.selected_input.to_ascii_uppercase().starts_with("HATSWITCH");
            if is_hat {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Direction:").color(theme.text_secondary).small());
                    for dir in ["NORTH", "NORTHEAST", "EAST", "SOUTHEAST", "SOUTH", "SOUTHWEST", "WEST", "NORTHWEST"] {
                        ui.selectable_value(&mut state.pov_dir, dir.to_string(), title_case(dir));
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Trigger:").color(theme.text_secondary).small());
                for (kind, label, tip) in TRIGGER_KINDS {
                    ui.selectable_value(&mut state.trigger_kind, kind.to_string(), label).on_hover_text(tip);
                }
            });
            state.selected_trigger = if is_hat {
                format!("POV_{}_{}", state.pov_dir, state.trigger_kind)
            } else {
                state.trigger_kind.clone()
            };

            ui.add_space(4.0);

            // Action type choices
            ui.horizontal(|ui| {
                ui.label(RichText::new("Action Type:").color(theme.text_secondary).small());
                ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::SimConnectCustom, "SimConnect Event");
                if !crate::pmdg::installed_sdks().is_empty() {
                    ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::Pmdg, "PMDG Event");
                }
                ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::Lvar, "MSFS LVAR");
                ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::CalculatorCode, "RPN Calculator");
            });

            ui.add_space(6.0);

            // Specific action parameters
            match state.new_action_type {
                ActionTypeChoice::Pmdg => render_pmdg_picker(ui, state, theme),
                ActionTypeChoice::SimConnectCustom => render_sim_event_picker(ui, state, theme),
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
        ActionTypeChoice::Pmdg => {
            let Some((target, param)) = pmdg_selection(state) else { return };
            EventActionItem::Control(EventActionControl { target, value: Some(param.to_string()), parameters: Vec::new() })
        }
        ActionTypeChoice::SimConnectCustom => {
            // Keep explicit providers (SIMCONNECT:, PMDGNG3:, ...) as typed.
            let target = if crate::profile::evaluator::split_data_prefix(state.custom_target.trim()).is_some() {
                state.custom_target.trim().to_string()
            } else {
                format!("SIMCONNECT:{}", state.custom_target.trim())
            };
            EventActionItem::Control(EventActionControl {
                target,
                value: Some(state.action_value.clone()),
                parameters: Vec::new(),
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

    // Same trigger, no condition: add to the existing definition (one definition per trigger)
    // instead of stacking IsFinal definitions where only the first would run.
    defs.items.retain(|d| d.actions.as_ref().is_some_and(|a| !a.items.is_empty()));
    if conditions.is_none() {
        if let Some(existing) = defs.items.iter_mut().find(|d| {
            d.conditions.as_ref().is_none_or(|c| c.items.is_empty())
                && crate::profile::executor::trigger_matches(d.trigger.as_deref().unwrap_or(""), &state.selected_trigger)
        }) {
            existing.actions.get_or_insert_with(|| crate::profile::schema::EventActionsList { items: Vec::new() }).items.push(action_item);
            return;
        }
    }
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
    invert: bool,
) {
    let action = EventActionItem::CustomAxis(crate::profile::schema::EventActionCustomAxis {
        target: format!("SIMCONNECT:{}", event_name),
        value: None,
        options: invert.then(|| crate::profile::schema::ProfileOptionsList {
            items: vec![crate::profile::schema::ProfileOption {
                key: "INVERTAXIS".to_string(),
                value: Some("True".to_string()),
            }],
        }),
    });
    set_value_definition(profile, conn_dev, bound_to, vec![action]);
}

/// Replaces the axis (`VALUE`) definitions of an input with one holding `actions`, keeping
/// any button-style definitions (`PRESSSHORT`, ...) bound to the same input.
fn set_value_definition(
    profile: &mut Option<SpadProfile>,
    conn_dev: &ConnectedDevice,
    bound_to: &str,
    actions: Vec<EventActionItem>,
) {
    let Some(ref mut prof) = profile else { return };
    let dev_prof = prof.ensure_device_for_connected(conn_dev.vendor_id, conn_dev.product_id, &conn_dev.id, &conn_dev.name, None);
    if dev_prof.pages.is_empty() {
        dev_prof.pages.push(crate::profile::schema::DevicePage {
            id: Some("Default".to_string()),
            is_default: Some(true),
            name: Some("Default".to_string()),
            events: Some(EventList { items: Vec::new() }),
        });
    }
    let events = dev_prof.pages[0].events.get_or_insert_with(|| EventList { items: Vec::new() });
    let idx = match events.items.iter().position(|e| crate::profile::schema::bound_to_matches(&e.bound_to, bound_to)) {
        Some(i) => i,
        None => {
            events.items.push(SpadEvent {
                bound_to: bound_to.to_string(),
                config_id: None,
                event_definitions: Some(EventDefinitionsList { items: Vec::new() }),
            });
            events.items.len() - 1
        }
    };
    let defs = events.items[idx].event_definitions.get_or_insert_with(|| EventDefinitionsList { items: Vec::new() });
    defs.items.retain(|d| !d.trigger.as_deref().unwrap_or("").eq_ignore_ascii_case("VALUE"));
    defs.items.insert(
        0,
        EventDefinition {
            trigger: Some("VALUE".to_string()),
            priority: None,
            is_final: Some(true),
            condition_binding: None,
            conditions: None,
            actions: Some(crate::profile::schema::EventActionsList { items: actions }),
        },
    );
}

/// Trigger kinds offered in the add-action form: (id, label, tooltip).
const TRIGGER_KINDS: [(&str, &str, &str); 5] = [
    ("PRESS", "Press", "Fires as soon as the button goes down"),
    ("RELEASE", "Release", "Fires when the button is let go"),
    ("PRESSSHORT", "Short press", "Fires on release if held less than half a second"),
    ("PRESSLONG", "Long press", "Fires once after holding for half a second"),
    ("HELD", "Held (repeat)", "Fires on press, then repeats every 100 ms while held — for trim"),
];

fn title_case(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut c = lower.chars();
    c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
}

/// Human label for a trigger id: `PRESSLONG` -> `Long press`, `POV_NORTH_HELD` -> `Hat North · Held (repeat)`.
pub fn trigger_label(trigger: &str) -> String {
    let t = trigger.to_ascii_uppercase();
    if let Some(rest) = t.strip_prefix("POV_") {
        if let Some((dir, kind)) = rest.split_once('_') {
            return format!("Hat {} · {}", title_case(dir), trigger_label(kind));
        }
    }
    match t.as_str() {
        "VALUE" => "Axis value".to_string(),
        _ => TRIGGER_KINDS
            .iter()
            .find(|(id, _, _)| *id == t)
            .map(|(_, l, _)| l.to_string())
            .unwrap_or_else(|| title_case(&t.replace('_', " "))),
    }
}

/// Human label for an input id: `BUTTON_5` -> `Button 5`, `ABS_RZ` / `RZ_AXIS` -> `Rz axis`.
pub fn input_label(id: &str) -> String {
    let u = id.to_ascii_uppercase();
    if let Some(n) = u.strip_prefix("BUTTON_") {
        return format!("Button {}", n);
    }
    if let Some(n) = u.strip_prefix("HATSWITCH_") {
        return format!("Hat switch {}", n);
    }
    let axis = u.strip_prefix("ABS_").or_else(|| u.strip_suffix("_AXIS")).or_else(|| u.strip_prefix("AXIS_"));
    match axis {
        Some("THROTTLE") => "Throttle axis".to_string(),
        Some("RUDDER") => "Rudder axis".to_string(),
        Some(a) if a.len() <= 2 => format!("{} axis", title_case(a)),
        _ => id.to_string(),
    }
}

/// True for analog axis names: Linux evdev (`ABS_X`, `ABS_RZ`, ...) and SPAD.neXt (`X_AXIS`, `SLIDER_0`, ...).
fn is_axis_input_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    (n.starts_with("ABS_") && !n.starts_with("ABS_HAT"))
        || n.contains("AXIS")
        || n.starts_with("SLIDER")
        || n.contains("THROTTLE")
        || n.contains("RUDDER")
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
                            // Drop definitions left without actions
                            if defs.items.get(def_idx).is_some_and(|d| d.actions.as_ref().is_none_or(|a| a.items.is_empty())) {
                                defs.items.remove(def_idx);
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
        ActionTypeChoice::Pmdg => {
            if let Some((target, param)) = pmdg_selection(state) {
                if let Some((prefix, name)) = target.split_once(':') {
                    let _ = cmd_tx.send(GuiCommand::FirePmdgEvent { prefix: prefix.to_string(), name: name.to_string(), parameter: param });
                }
            }
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

/// Searchable SimConnect event picker; the name field doubles as the search box and still
/// accepts any event name typed by hand.
fn render_sim_event_picker(ui: &mut Ui, state: &mut MappingViewState, theme: &AeroTheme) {
    use crate::gui::sim_events::{categories, SIM_EVENTS};
    ui.horizontal(|ui| {
        ui.label(RichText::new("Event:").color(theme.text_secondary).small());
        ui.add(
            egui::TextEdit::singleline(&mut state.custom_target)
                .hint_text("🔍 Search or type, e.g. GEAR or AP_MASTER")
                .desired_width(320.0),
        );
        ui.label(RichText::new("Value:").color(theme.text_secondary).small());
        ui.add(egui::TextEdit::singleline(&mut state.action_value).desired_width(60.0));
    });
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(state.sim_event_category.is_none(), "All").clicked() {
            state.sim_event_category = None;
        }
        for cat in categories() {
            if ui.selectable_label(state.sim_event_category == Some(cat), cat).clicked() {
                state.sim_event_category = Some(cat);
            }
        }
    });
    let query = state.custom_target.trim().trim_start_matches("SIMCONNECT:").to_ascii_uppercase();
    let words: Vec<&str> = query.split(|c: char| c.is_whitespace() || c == '_').filter(|w| !w.is_empty()).collect();
    let exact = SIM_EVENTS.iter().any(|e| e.name == query);
    let list: Vec<&crate::gui::sim_events::SimEvent> = SIM_EVENTS
        .iter()
        .filter(|e| state.sim_event_category.is_none_or(|c| c == e.category))
        .filter(|e| exact || words.iter().all(|w| e.name.contains(w)))
        .collect();
    egui::Frame::none().fill(theme.bg_base).rounding(6.0).inner_margin(4.0).show(ui, |ui| {
        ScrollArea::vertical().id_salt("sim_event_list").max_height(200.0).auto_shrink([false, true]).show(ui, |ui| {
            if list.is_empty() {
                ui.label(
                    RichText::new("Not in the catalog: the name you typed will be sent as-is.")
                        .color(theme.text_muted)
                        .small()
                        .italics(),
                );
            }
            for e in list {
                ui.horizontal(|ui| {
                    if ui.selectable_label(e.name == query, RichText::new(e.name).monospace().small()).clicked() {
                        state.custom_target = e.name.to_string();
                    }
                    ui.label(RichText::new(e.category).color(theme.text_muted).small());
                });
            }
        });
    });
}

/// Selected PMDG target (`PMDGNG3:EVT_...`) and parameter, if an event is picked.
fn pmdg_selection(state: &MappingViewState) -> Option<(String, u32)> {
    let sdk = crate::pmdg::installed_sdks().get(state.pmdg_family_idx)?;
    if state.pmdg_event.is_empty() {
        return None;
    }
    let param = PMDG_PARAMS.get(state.pmdg_param_idx).map(|p| p.1).unwrap_or(crate::pmdg::MOUSE_FLAG_LEFTSINGLE);
    Some((format!("{}:{}", sdk.family.profile_prefixes()[0], state.pmdg_event), param))
}

/// Searchable list of the events in the installed PMDG SDK header.
fn render_pmdg_picker(ui: &mut Ui, state: &mut MappingViewState, theme: &AeroTheme) {
    let sdks = crate::pmdg::installed_sdks();
    if state.pmdg_family_idx >= sdks.len() {
        state.pmdg_family_idx = 0;
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Aircraft:").color(theme.text_secondary).small());
        for (i, sdk) in sdks.iter().enumerate() {
            if ui.selectable_value(&mut state.pmdg_family_idx, i, sdk.family.label()).changed() {
                state.pmdg_event.clear();
            }
        }
    });
    let Some(sdk) = sdks.get(state.pmdg_family_idx) else { return };
    ui.add(
        egui::TextEdit::singleline(&mut state.pmdg_query)
            .hint_text("🔍 Search events, e.g. MCP HEADING or BATTERY")
            .desired_width(f32::INFINITY),
    );
    let words: Vec<String> = state.pmdg_query.split_whitespace().map(|w| w.to_ascii_uppercase()).collect();
    let mut names: Vec<&String> = sdk
        .events
        .keys()
        .filter(|n| words.iter().all(|w| n.contains(w.as_str())))
        .collect();
    names.sort();
    ui.label(RichText::new(format!("{} events", names.len())).color(theme.text_muted).small());
    egui::Frame::none().fill(theme.bg_base).rounding(6.0).inner_margin(4.0).show(ui, |ui| {
        ScrollArea::vertical().id_salt("pmdg_events").max_height(170.0).auto_shrink([false, true]).show_rows(ui, 20.0, names.len(), |ui, range| {
            for name in &names[range] {
                let selected = state.pmdg_event == **name;
                if ui.selectable_label(selected, RichText::new(name.as_str()).monospace().small()).clicked() {
                    state.pmdg_event = (*name).clone();
                }
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Parameter:").color(theme.text_secondary).small());
        for (i, (label, _)) in PMDG_PARAMS.iter().enumerate() {
            ui.selectable_value(&mut state.pmdg_param_idx, i, *label);
        }
    });
    if !state.pmdg_event.is_empty() {
        ui.label(RichText::new(format!("Selected: {}", state.pmdg_event)).color(theme.accent).small().strong());
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
