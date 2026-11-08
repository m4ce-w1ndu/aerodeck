use eframe::egui::{self, Color32, RichText, Ui, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::gui::app::LiveAxisState;
use crate::gui::backend::{GuiCommand, TargetDeviceSpec};
use crate::gui::presets::{COMMON_CALCULATOR_CODES, SIMCONNECT_AXIS_PRESETS, SIMCONNECT_PRESETS};
use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::schema::{
    EventActionControl, EventActionItem, EventActionScript, EventConditionItem,
    EventConditionSimple, EventConditionsList, EventDefinition, EventDefinitionsList, EventList,
    SpadEvent, SpadProfile,
};
use crate::profile::{AircraftAssignmentManager, DiscoveredAircraft, ProfileRepository};

pub struct MappingViewState {
    pub selected_device_idx: usize,
    pub selected_input: String,
    pub filter_query: String,
    pub follow_active_input: bool,
    pub input_type_filter: InputFilterType,

    // Profile & Aircraft bar state in Mapping view
    pub selected_aircraft_to_bind_idx: usize,
    pub show_merge_device_panel: bool,
    pub selected_merge_profile_idx: usize,
    pub new_profile_name_input: String,
    pub show_new_profile_modal: bool,

    // Axis calibration & options
    pub axis_invert: bool,
    pub axis_deadzone_pct: f32,
    pub selected_axis_preset_idx: usize,

    // Add action form state (Buttons/Switches)
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
            selected_aircraft_to_bind_idx: 0,
            show_merge_device_panel: false,
            selected_merge_profile_idx: 0,
            new_profile_name_input: "My Custom Multi-Device Profile".to_string(),
            show_new_profile_modal: false,
            axis_invert: false,
            axis_deadzone_pct: 1.0,
            selected_axis_preset_idx: 0,
            new_action_type: ActionTypeChoice::SimConnectPreset,
            selected_preset_idx: 0,
            custom_target: "SIMCONNECT:TOGGLE_MASTER_BATTERY".to_string(),
            action_value: "1".to_string(),
            selected_trigger: "PRESS".to_string(),
            use_condition: false,
            cond_target: "SIMCONNECT:ELECTRICAL MASTER BATTERY".to_string(),
            cond_comp: "Equals".to_string(),
            cond_value: "0".to_string(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_mapping_view(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_name: &mut String,
    profile_path_input: &mut String,
    active_aircraft_name: &str,
    discovered_aircrafts: &[DiscoveredAircraft],
    aircraft_assignments: &AircraftAssignmentManager,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    last_input: &Option<(String, String, String, Instant)>,
    discovered_lvars: &[String],
    devices: &[ConnectedDevice],
    live_axes: &HashMap<String, LiveAxisState>,
) {
    // 1. TOP BANNER: Active Multi-Device Profile, Aircraft Binding & Per-Device Status
    render_active_profile_and_devices_bar(
        ui,
        state,
        profile,
        profile_name,
        profile_path_input,
        active_aircraft_name,
        discovered_aircrafts,
        aircraft_assignments,
        devices,
        cmd_tx,
    );

    ui.add_space(6.0);

    // 2. LIVE HARDWARE INPUT MONITOR
    render_live_input_banner(ui, state, last_input, devices);

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // 3. MAIN 2-COLUMN SPLIT: Left = Device & Input List, Right = Mapping & Action Editor
    ui.columns(2, |columns| {
        // --- LEFT COLUMN: Connected Hardware Device selector & Inputs ---
        render_device_and_input_column(
            &mut columns[0],
            state,
            profile,
            last_input,
            devices,
            cmd_tx,
            live_axes,
        );

        // --- RIGHT COLUMN: Mappings & Action Configurator ---
        render_action_editor_column(
            &mut columns[1],
            state,
            profile,
            profile_path_input,
            cmd_tx,
            discovered_lvars,
            devices,
            live_axes,
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn render_active_profile_and_devices_bar(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_name: &mut String,
    profile_path_input: &mut String,
    active_aircraft_name: &str,
    discovered_aircrafts: &[DiscoveredAircraft],
    aircraft_assignments: &AircraftAssignmentManager,
    devices: &[ConnectedDevice],
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    let local_profiles = ProfileRepository::list_local_profiles();

    egui::Frame::none()
        .fill(Color32::from_rgb(20, 28, 40))
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(55, 85, 130)))
        .rounding(6.0)
        .inner_margin(Vec2::new(10.0, 8.0))
        .show(ui, |ui| {
            // Row 1: Loaded Multi-Device Profile + Switcher + Bound Aircrafts + Quick Bind
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("📂 LOADED PROFILE:")
                        .color(Color32::from_rgb(130, 200, 255))
                        .strong(),
                );

                let display_prof_name = profile
                    .as_ref()
                    .and_then(|p| p.name.as_deref())
                    .unwrap_or(profile_name.as_str());

                egui::Frame::none()
                    .fill(Color32::from_rgb(15, 55, 40))
                    .rounding(4.0)
                    .inner_margin(Vec2::new(8.0, 3.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "✔ {}  ({})",
                                display_prof_name, profile_path_input
                            ))
                            .color(Color32::from_rgb(80, 255, 160))
                            .strong(),
                        );
                    });

                // Quick Profile Switcher Dropdown
                egui::ComboBox::from_id_salt("mapping_view_profile_switcher")
                    .width(210.0)
                    .selected_text("🔄 Switch Loaded Profile...")
                    .show_ui(ui, |ui| {
                        for path in &local_profiles {
                            let is_cur = path == profile_path_input;
                            if ui.selectable_label(is_cur, path).clicked() {
                                *profile_path_input = path.clone();
                                let _ = cmd_tx.send(GuiCommand::LoadProfile(PathBuf::from(path)));
                            }
                        }
                    });

                if ui
                    .button(
                        RichText::new("➕ New Multi-Device Profile")
                            .color(Color32::from_rgb(100, 220, 255)),
                    )
                    .on_hover_text("Create a clean multi-device profile that holds mappings for all your controllers")
                    .clicked()
                {
                    state.show_new_profile_modal = !state.show_new_profile_modal;
                }

                ui.separator();

                // Show which aircraft(s) are bound to this profile
                let bound_aircrafts: Vec<&String> = aircraft_assignments
                    .assignments
                    .iter()
                    .filter(|(_, p)| {
                        *p == profile_path_input
                            || profile_path_input.ends_with(p.as_str())
                            || p.ends_with(profile_path_input.as_str())
                    })
                    .map(|(ac, _)| ac)
                    .collect();

                if !bound_aircrafts.is_empty() {
                    let ac_list = if bound_aircrafts.len() <= 2 {
                        bound_aircrafts
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        format!(
                            "{}, +{} more",
                            bound_aircrafts[0],
                            bound_aircrafts.len() - 1
                        )
                    };
                    ui.label(
                        RichText::new(format!("✈ Bound to Aircraft: {}", ac_list))
                            .color(Color32::from_rgb(255, 215, 80))
                            .strong(),
                    );
                } else {
                    ui.label(
                        RichText::new("✈ Not yet bound to an aircraft")
                            .color(Color32::GRAY)
                            .italics(),
                    );
                }

                // Quick Bind Current Profile to Aircraft right from Mapping screen
                if !discovered_aircrafts.is_empty() {
                    if state.selected_aircraft_to_bind_idx >= discovered_aircrafts.len() {
                        state.selected_aircraft_to_bind_idx = 0;
                    }
                    let sel_ac_title =
                        &discovered_aircrafts[state.selected_aircraft_to_bind_idx].title;
                    egui::ComboBox::from_id_salt("mapping_quick_bind_ac_combo")
                        .width(190.0)
                        .selected_text(sel_ac_title)
                        .show_ui(ui, |ui| {
                            for (idx, ac) in discovered_aircrafts.iter().enumerate() {
                                ui.selectable_value(
                                    &mut state.selected_aircraft_to_bind_idx,
                                    idx,
                                    &ac.title,
                                );
                            }
                        });
                    if ui
                        .button(
                            RichText::new("🔗 Bind Profile to Aircraft")
                                .color(Color32::from_rgb(80, 255, 140))
                                .strong(),
                        )
                        .on_hover_text("Bind this multi-device profile to the selected aircraft so it loads automatically")
                        .clicked()
                    {
                        let _ = cmd_tx.send(GuiCommand::BindAircraftToProfile {
                            aircraft_title: sel_ac_title.clone(),
                            profile_path: profile_path_input.clone(),
                        });
                    }
                } else if !active_aircraft_name.is_empty()
                    && active_aircraft_name != "No Aircraft Detected"
                {
                    if ui
                        .button(
                            RichText::new(format!("🔗 Bind to '{}'", active_aircraft_name))
                                .color(Color32::from_rgb(80, 255, 140))
                                .strong(),
                        )
                        .clicked()
                    {
                        let _ = cmd_tx.send(GuiCommand::BindAircraftToProfile {
                            aircraft_title: active_aircraft_name.to_string(),
                            profile_path: profile_path_input.clone(),
                        });
                    }
                }
            });

            // Optional New Profile creation row
            if state.show_new_profile_modal {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("New Profile Name:").strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_profile_name_input)
                            .desired_width(240.0),
                    );
                    if ui
                        .button(
                            RichText::new("✔ Create & Activate Empty Multi-Device Profile")
                                .color(Color32::from_rgb(80, 255, 140))
                                .strong(),
                        )
                        .clicked()
                    {
                        let slug: String = state
                            .new_profile_name_input
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                            .collect();
                        let new_path = format!("profiles/{}.xml", slug.trim_matches('_'));
                        let new_prof = SpadProfile {
                            version: Some(2),
                            is_stock_profile: Some(false),
                            name: Some(state.new_profile_name_input.clone()),
                            variables: None,
                            options: None,
                            assigned_aircrafts: None,
                            devices: Some(crate::profile::schema::DeviceList { items: Vec::new() }),
                            profile_info: None,
                        };
                        *profile = Some(new_prof.clone());
                        *profile_name = state.new_profile_name_input.clone();
                        *profile_path_input = new_path.clone();
                        let _ = cmd_tx.send(GuiCommand::UpdateAndSaveProfile {
                            profile: new_prof,
                            path: PathBuf::from(new_path),
                            bind_to_aircraft: None,
                        });
                        state.show_new_profile_modal = false;
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_new_profile_modal = false;
                    }
                });
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            // Row 2: Multi-Device Container Overview — Shows every connected device and how many mappings it has in THIS profile!
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("🎮 DEVICES IN THIS PROFILE (Click a controller to view/edit its mappings):")
                        .color(Color32::LIGHT_GRAY)
                        .small()
                        .strong(),
                );

                for (idx, conn_dev) in devices.iter().enumerate() {
                    let is_selected = state.selected_device_idx == idx;
                    let (mapped_inputs, total_acts) = profile
                        .as_ref()
                        .and_then(|p| {
                            p.find_device_for_connected(
                                conn_dev.vendor_id,
                                conn_dev.product_id,
                                &conn_dev.id,
                            )
                        })
                        .map(|d| (d.mapped_input_count(), d.total_action_count()))
                        .unwrap_or((0, 0));

                    let status_text = if mapped_inputs > 0 {
                        format!(
                            "✔ {} ({:04X}:{:04X}) [{} inputs / {} actions]",
                            conn_dev.name,
                            conn_dev.vendor_id,
                            conn_dev.product_id,
                            mapped_inputs,
                            total_acts
                        )
                    } else {
                        format!(
                            "○ {} ({:04X}:{:04X}) [0 mapped — click to configure]",
                            conn_dev.name, conn_dev.vendor_id, conn_dev.product_id
                        )
                    };

                    let btn_color = if is_selected {
                        Color32::from_rgb(0, 230, 255)
                    } else if mapped_inputs > 0 {
                        Color32::from_rgb(100, 255, 140)
                    } else {
                        Color32::GRAY
                    };

                    if ui
                        .selectable_label(
                            is_selected,
                            RichText::new(status_text).color(btn_color).small().strong(),
                        )
                        .clicked()
                    {
                        state.selected_device_idx = idx;
                        if let Some(first_inp) = conn_dev.inputs.first() {
                            state.selected_input = first_inp.clone();
                        }
                    }
                }

                // Also show any additional un-connected devices stored inside the profile XML
                if let Some(ref prof) = profile {
                    if let Some(ref devs) = prof.devices {
                        for d in &devs.items {
                            let acts = d.total_action_count();
                            if acts == 0 {
                                continue;
                            }
                            let matches_connected = devices.iter().any(|cd| {
                                d.matches_vid_pid(cd.vendor_id, cd.product_id)
                                    || d.matches_device_id(&cd.id)
                            });
                            if !matches_connected {
                                ui.label(
                                    RichText::new(format!(
                                        "📦 {} [{} actions, unplugged]",
                                        d.friendly_device_name(),
                                        acts
                                    ))
                                    .color(Color32::from_rgb(180, 160, 120))
                                    .small(),
                                );
                            }
                        }
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let active_dev_name = devices
                        .get(state.selected_device_idx)
                        .map(|d| d.name.as_str())
                        .unwrap_or("Selected Device");
                    if ui
                        .button(
                            RichText::new(format!(
                                "📥 Import / Merge Device Profile into '{}'",
                                active_dev_name
                            ))
                            .color(Color32::from_rgb(255, 210, 90))
                            .small()
                            .strong(),
                        )
                        .on_hover_text(
                            "Import device mappings from another profile into this specific controller while keeping all other controllers in the current profile intact!",
                        )
                        .clicked()
                    {
                        state.show_merge_device_panel = !state.show_merge_device_panel;
                    }
                });
            });

            // Optional Row 3: Merge Device Mappings into Active Device panel
            if state.show_merge_device_panel {
                if let Some(active_dev) = devices.get(state.selected_device_idx) {
                    ui.add_space(4.0);
                    egui::Frame::none()
                        .fill(Color32::from_rgb(30, 38, 52))
                        .rounding(4.0)
                        .inner_margin(Vec2::new(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "➕ Merge Device Mappings into '{}' ({:04X}:{:04X}) inside Current Profile:",
                                        active_dev.name, active_dev.vendor_id, active_dev.product_id
                                    ))
                                    .color(Color32::from_rgb(255, 215, 100))
                                    .strong(),
                                );

                                if !local_profiles.is_empty() {
                                    if state.selected_merge_profile_idx >= local_profiles.len() {
                                        state.selected_merge_profile_idx = 0;
                                    }
                                    let sel_path =
                                        &local_profiles[state.selected_merge_profile_idx];
                                    egui::ComboBox::from_id_salt("merge_source_profile_combo")
                                        .width(260.0)
                                        .selected_text(sel_path)
                                        .show_ui(ui, |ui| {
                                            for (idx, p) in local_profiles.iter().enumerate() {
                                                ui.selectable_value(
                                                    &mut state.selected_merge_profile_idx,
                                                    idx,
                                                    p,
                                                );
                                            }
                                        });

                                    if ui
                                        .button(
                                            RichText::new(format!(
                                                "📥 Merge into '{}' & Keep Other Devices",
                                                active_dev.name
                                            ))
                                            .color(Color32::from_rgb(80, 255, 140))
                                            .strong(),
                                        )
                                        .clicked()
                                    {
                                        let _ = cmd_tx.send(GuiCommand::MergeProfileFromFile {
                                            source_path: PathBuf::from(sel_path),
                                            target_device: Some(TargetDeviceSpec {
                                                vendor_id: active_dev.vendor_id,
                                                product_id: active_dev.product_id,
                                                device_id: active_dev.id.clone(),
                                                device_name: active_dev.name.clone(),
                                            }),
                                            bind_to_aircraft: None,
                                        });
                                        state.show_merge_device_panel = false;
                                    }
                                }

                                if ui.button("📂 Browse XML File to Merge...").clicked() {
                                    if let Some(file) = rfd::FileDialog::new()
                                        .add_filter("SPAD.neXt Profile", &["xml", "spadprofile"])
                                        .pick_file()
                                    {
                                        let _ = cmd_tx.send(GuiCommand::MergeProfileFromFile {
                                            source_path: file,
                                            target_device: Some(TargetDeviceSpec {
                                                vendor_id: active_dev.vendor_id,
                                                product_id: active_dev.product_id,
                                                device_id: active_dev.id.clone(),
                                                device_name: active_dev.name.clone(),
                                            }),
                                            bind_to_aircraft: None,
                                        });
                                        state.show_merge_device_panel = false;
                                    }
                                }

                                if ui.button("Close").clicked() {
                                    state.show_merge_device_panel = false;
                                }
                            });
                            ui.label(
                                RichText::new(
                                    "Tip: You can also import directly from the '✈ Aircraft Bindings & Profile Repository' tab using '➕ Merge into Active Profile' or by selecting a Target Device.",
                                )
                                .color(Color32::GRAY)
                                .small(),
                            );
                        });
                }
            }
        });
}

fn render_live_input_banner(
    ui: &mut Ui,
    state: &mut MappingViewState,
    last_input: &Option<(String, String, String, Instant)>,
    devices: &[ConnectedDevice],
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("HARDWARE INPUT MONITOR:").strong());

        if let Some((dev, bound, act, inst)) = last_input {
            let elapsed_ms = inst.elapsed().as_millis();
            let is_recent = elapsed_ms < 1500;
            let bg_color = if is_recent {
                Color32::from_rgb(0, 110, 40)
            } else {
                Color32::from_rgb(40, 45, 55)
            };

            egui::Frame::none()
                .fill(bg_color)
                .rounding(4.0)
                .inner_margin(Vec2::new(8.0, 4.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("● [{}] {} -> {}", dev, bound, act))
                            .color(if is_recent {
                                Color32::from_rgb(100, 255, 100)
                            } else {
                                Color32::LIGHT_GRAY
                            })
                            .strong(),
                    );
                });

            // Follow active input if enabled
            if state.follow_active_input && is_recent && elapsed_ms < 200 {
                for (idx, d) in devices.iter().enumerate() {
                    if d.name == dev.as_str() || d.id == dev.as_str() {
                        state.selected_device_idx = idx;
                        state.selected_input = bound.clone();
                        break;
                    }
                }
            }
        } else {
            ui.label(
                RichText::new(
                    "Awaiting controller input... Move a switch, press a button, or move an axis on your physical hardware",
                )
                .italics()
                .color(Color32::GRAY),
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(
                &mut state.follow_active_input,
                "Auto-Select Active Hardware Input",
            );
        });
    });
}

fn render_device_and_input_column(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    last_input: &Option<(String, String, String, Instant)>,
    devices: &[ConnectedDevice],
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    live_axes: &HashMap<String, LiveAxisState>,
) {
    ui.heading(
        RichText::new("Connected Flight Hardware & Inputs").color(Color32::from_rgb(100, 180, 255)),
    );
    ui.add_space(4.0);

    if devices.is_empty() {
        ui.label(
            RichText::new(
                "No controllers detected. Plug in your joystick, throttle, or yoke and click Rescan.",
            )
            .color(Color32::LIGHT_RED),
        );
        if ui.button("🔄 Rescan Controllers").clicked() {
            let _ = cmd_tx.send(GuiCommand::RescanDevices);
        }
        return;
    }

    if state.selected_device_idx >= devices.len() {
        state.selected_device_idx = 0;
    }

    let active_device = &devices[state.selected_device_idx];

    // Ensure selected input is valid for this device
    if (state.selected_input.is_empty() || !active_device.inputs.contains(&state.selected_input))
        && !active_device.inputs.is_empty()
    {
        state.selected_input = active_device.inputs[0].clone();
    }

    // Device selector dropdown & Rescan button
    ui.horizontal(|ui| {
        ui.label("Editing Device:");
        let dev_mapped_count = profile
            .as_ref()
            .and_then(|p| {
                p.find_device_for_connected(
                    active_device.vendor_id,
                    active_device.product_id,
                    &active_device.id,
                )
            })
            .map(|d| d.mapped_input_count())
            .unwrap_or(0);

        let selected_text = format!(
            "{} [{}] ({} mapped)",
            active_device.name,
            active_device.category.badge(),
            dev_mapped_count
        );
        egui::ComboBox::from_id_salt("connected_device_combo")
            .width(320.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (idx, dev) in devices.iter().enumerate() {
                    let mapped_cnt = profile
                        .as_ref()
                        .and_then(|p| {
                            p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id)
                        })
                        .map(|d| d.mapped_input_count())
                        .unwrap_or(0);
                    let label = format!(
                        "{} [{}] — {} mapped inputs in profile",
                        dev.name,
                        dev.category.badge(),
                        mapped_cnt
                    );
                    if ui
                        .selectable_value(&mut state.selected_device_idx, idx, label)
                        .clicked()
                    {
                        if !dev.inputs.is_empty() {
                            state.selected_input = dev.inputs[0].clone();
                        }
                    }
                }
            });

        if ui.button("🔄 Rescan").clicked() {
            let _ = cmd_tx.send(GuiCommand::RescanDevices);
        }
    });

    ui.add_space(4.0);

    // Device technical info pill
    egui::Frame::none()
        .fill(Color32::from_rgb(25, 30, 38))
        .rounding(3.0)
        .inner_margin(Vec2::new(6.0, 3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "VID: {:04X}  PID: {:04X}",
                        active_device.vendor_id, active_device.product_id
                    ))
                    .color(Color32::LIGHT_GRAY)
                    .small(),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!("{} Buttons", active_device.buttons.len()))
                        .color(Color32::from_rgb(0, 220, 150))
                        .small(),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!("{} Axes", active_device.axes.len()))
                        .color(Color32::from_rgb(0, 200, 255))
                        .small(),
                );
                if let Some(ref p) = active_device.path {
                    ui.separator();
                    ui.label(RichText::new(format!("{:?}", p)).color(Color32::GRAY).small());
                }
            });
        });

    ui.add_space(6.0);

    // Filter controls: Type filter (All / Buttons / Axes / Mapped Only) + Search box
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.input_type_filter, InputFilterType::All, "All");
        ui.selectable_value(
            &mut state.input_type_filter,
            InputFilterType::Buttons,
            format!("Buttons ({})", active_device.buttons.len()),
        );
        ui.selectable_value(
            &mut state.input_type_filter,
            InputFilterType::Axes,
            format!("Axes ({})", active_device.axes.len()),
        );
        ui.selectable_value(
            &mut state.input_type_filter,
            InputFilterType::MappedOnly,
            "★ Mapped",
        );

        ui.separator();

        ui.label("Search:");
        ui.add(egui::TextEdit::singleline(&mut state.filter_query).desired_width(90.0));
        if ui.button("✖").clicked() {
            state.filter_query.clear();
        }
    });

    ui.add_space(4.0);

    let inputs_to_display: Vec<&String> = match state.input_type_filter {
        InputFilterType::All | InputFilterType::MappedOnly => active_device.inputs.iter().collect(),
        InputFilterType::Buttons => active_device.buttons.iter().collect(),
        InputFilterType::Axes => active_device.axes.iter().collect(),
    };

    // Scrollable input list scoped strictly to `active_device`
    egui::ScrollArea::vertical()
        .max_height(430.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            for inp in inputs_to_display {
                if !state.filter_query.is_empty()
                    && !inp
                        .to_lowercase()
                        .contains(&state.filter_query.to_lowercase())
                {
                    continue;
                }

                // Count mapped actions strictly for THIS active_device + input in the profile
                let mapped_count = count_actions_for_device_input(profile, active_device, inp);
                if state.input_type_filter == InputFilterType::MappedOnly && mapped_count == 0 {
                    continue;
                }

                let is_selected = &state.selected_input == inp;
                let is_last_active = last_input
                    .as_ref()
                    .map(|(dev_name, b, _, inst)| {
                        (dev_name == &active_device.name || dev_name == &active_device.id)
                            && b == inp
                            && inst.elapsed().as_millis() < 2000
                    })
                    .unwrap_or(false);

                let is_axis = active_device.axes.contains(inp);
                let axis_str = if let Some(ax) = live_axes
                    .get(&format!("{}:{}", active_device.name, inp))
                    .or_else(|| live_axes.get(inp))
                {
                    format!("⚡ {} ({:+.0}%)", inp, ax.normalized * 100.0)
                } else if is_axis {
                    format!("⚡ {}", inp)
                } else {
                    format!("🔘 {}", inp)
                };

                let label_text = if mapped_count > 0 {
                    format!(
                        "{}  [{} action{}]",
                        axis_str,
                        mapped_count,
                        if mapped_count == 1 { "" } else { "s" }
                    )
                } else {
                    axis_str
                };

                let mut text = RichText::new(label_text);
                if is_last_active {
                    text = text.color(Color32::from_rgb(0, 255, 120)).strong();
                } else if mapped_count > 0 {
                    text = text.color(Color32::from_rgb(255, 215, 0));
                }

                if ui.selectable_label(is_selected, text).clicked() {
                    state.selected_input = inp.to_string();
                }
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn render_action_editor_column(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    discovered_lvars: &[String],
    devices: &[ConnectedDevice],
    live_axes: &HashMap<String, LiveAxisState>,
) {
    let display_input = if state.selected_input.is_empty() {
        "Select an input on the left"
    } else {
        &state.selected_input
    };

    let active_dev = devices.get(state.selected_device_idx);
    let is_axis = active_dev
        .map(|d| d.axes.iter().any(|a| a == &state.selected_input))
        .unwrap_or(false);

    ui.horizontal_wrapped(|ui| {
        if let Some(d) = active_dev {
            ui.label(
                RichText::new(format!("[{}]", d.name))
                    .color(Color32::from_rgb(120, 200, 255))
                    .strong(),
            );
        }
        ui.heading(
            RichText::new(format!("Input: {}", display_input))
                .color(Color32::from_rgb(255, 200, 100)),
        );
        if is_axis {
            ui.label(
                RichText::new("⚡ ANALOG AXIS")
                    .color(Color32::from_rgb(0, 220, 255))
                    .strong(),
            );
        }
    });
    ui.add_space(4.0);

    // 1. Current configured bindings list for THIS device + input
    render_current_bindings_group(
        ui,
        state,
        profile,
        profile_path_input,
        active_dev,
        cmd_tx,
    );

    ui.add_space(8.0);

    // 2. Specialized editor: Axis Telemetry & Presets OR Button/Switch Form
    if is_axis {
        render_axis_editor(
            ui,
            state,
            profile,
            profile_path_input,
            cmd_tx,
            devices,
            live_axes,
        );
    } else {
        render_button_editor(
            ui,
            state,
            profile,
            profile_path_input,
            cmd_tx,
            discovered_lvars,
            devices,
        );
    }
}

fn render_current_bindings_group(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    active_dev: Option<&ConnectedDevice>,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    ui.group(|ui| {
        let dev_title = active_dev
            .map(|d| format!("Configured Actions on '{}' -> '{}':", d.name, state.selected_input))
            .unwrap_or_else(|| "Configured Profile Actions:".to_string());
        ui.label(RichText::new(dev_title).strong());
        ui.add_space(2.0);

        let mut to_remove: Option<(usize, usize)> = None;
        let mut to_test: Option<GuiCommand> = None;
        let mut found_any = false;

        if let (Some(prof), Some(conn_dev)) = (profile.as_ref(), active_dev) {
            if let Some(dev_prof) = prof.find_device_for_connected(
                conn_dev.vendor_id,
                conn_dev.product_id,
                &conn_dev.id,
            ) {
                for page in &dev_prof.pages {
                    if let Some(ref evs) = page.events {
                        for ev in &evs.items {
                            if crate::profile::schema::bound_to_matches(
                                &ev.bound_to,
                                &state.selected_input,
                            ) {
                                if let Some(ref defs) = ev.event_definitions {
                                    for (def_idx, def) in defs.items.iter().enumerate() {
                                        let trigger = def.trigger.as_deref().unwrap_or("ALL");
                                        if let Some(ref acts) = def.actions {
                                            for (act_idx, act) in acts.items.iter().enumerate() {
                                                found_any = true;
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(format!("[{}]", trigger))
                                                            .color(Color32::LIGHT_BLUE)
                                                            .strong(),
                                                    );
                                                    match act {
                                                        EventActionItem::Control(c) => {
                                                            ui.label(format!(
                                                                "⚙ Control: {} (val: {:?})",
                                                                c.target, c.value
                                                            ));
                                                            if ui.button("Test").clicked() {
                                                                let val = c
                                                                    .value
                                                                    .as_deref()
                                                                    .and_then(|v| {
                                                                        v.parse::<u32>().ok()
                                                                    })
                                                                    .unwrap_or(1);
                                                                let name = c
                                                                    .target
                                                                    .strip_prefix("SIMCONNECT:")
                                                                    .or_else(|| {
                                                                        c.target
                                                                            .strip_prefix("MSFS:")
                                                                    })
                                                                    .unwrap_or(&c.target)
                                                                    .to_string();
                                                                to_test = Some(
                                                                    GuiCommand::FireSimConnectEvent {
                                                                        event_name: name,
                                                                        value: val,
                                                                    },
                                                                );
                                                            }
                                                        }
                                                        EventActionItem::CustomAxis(ca) => {
                                                            ui.label(format!(
                                                                "⚡ CustomAxis: {}",
                                                                ca.target
                                                            ));
                                                        }
                                                        EventActionItem::ChangeValue(cv) => {
                                                            ui.label(format!(
                                                                "📝 LVAR: {} = {:?}",
                                                                cv.target, cv.value
                                                            ));
                                                            if ui.button("Test").clicked() {
                                                                let val = cv
                                                                    .value
                                                                    .as_deref()
                                                                    .and_then(|v| {
                                                                        v.parse::<f64>().ok()
                                                                    })
                                                                    .unwrap_or(1.0);
                                                                let name = cv
                                                                    .target
                                                                    .strip_prefix("LVAR:")
                                                                    .unwrap_or(&cv.target)
                                                                    .to_string();
                                                                to_test =
                                                                    Some(GuiCommand::SetLvar {
                                                                        name,
                                                                        value: val,
                                                                    });
                                                            }
                                                        }
                                                        EventActionItem::Script(sc) => {
                                                            let script_text = sc
                                                                .script
                                                                .clone()
                                                                .unwrap_or_default();
                                                            ui.label(format!(
                                                                "💻 RPN: {}",
                                                                script_text
                                                            ));
                                                            if ui.button("Test").clicked() {
                                                                to_test = Some(
                                                                    GuiCommand::ExecCalculatorCode {
                                                                        code: script_text,
                                                                    },
                                                                );
                                                            }
                                                        }
                                                        _ => {
                                                            ui.label("Other action");
                                                        }
                                                    }

                                                    if ui
                                                        .button(
                                                            RichText::new("🗑 Delete")
                                                                .color(Color32::LIGHT_RED),
                                                        )
                                                        .clicked()
                                                    {
                                                        to_remove = Some((def_idx, act_idx));
                                                    }
                                                });
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

        if !found_any {
            ui.label(
                RichText::new(
                    "No actions bound to this input on the selected controller yet. Add one below — it will be saved into the multi-device profile automatically.",
                )
                .italics()
                .color(Color32::GRAY),
            );
        }

        if let Some(cmd) = to_test {
            let _ = cmd_tx.send(cmd);
        }

        if let (Some((def_idx, act_idx)), Some(conn_dev)) = (to_remove, active_dev) {
            remove_action_from_device_in_profile(
                profile,
                conn_dev,
                &state.selected_input,
                def_idx,
                act_idx,
            );
            sync_and_save_profile(profile, profile_path_input, cmd_tx);
        }
    });
}

fn render_axis_editor(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    devices: &[ConnectedDevice],
    live_axes: &HashMap<String, LiveAxisState>,
) {
    let active_dev = match devices.get(state.selected_device_idx) {
        Some(d) => d,
        None => return,
    };

    // --- Live Telemetry & Calibration Section ---
    ui.group(|ui| {
        ui.heading(
            RichText::new("Live Axis Telemetry & Calibration")
                .color(Color32::from_rgb(0, 220, 255)),
        );
        ui.add_space(4.0);

        let maybe_ax = live_axes
            .get(&format!("{}:{}", active_dev.name, state.selected_input))
            .or_else(|| live_axes.get(&state.selected_input));

        let (norm_val, raw_val, min_val, max_val) = if let Some(ax) = maybe_ax {
            (ax.normalized, ax.raw, ax.min, ax.max)
        } else {
            let (min, max) = active_dev
                .axis_ranges
                .get(&state.selected_input)
                .copied()
                .unwrap_or((0, 65535));
            (0.0, min, min, max)
        };

        let is_bipolar = min_val < 0;

        let effective_norm = if state.axis_invert {
            if is_bipolar {
                -norm_val
            } else {
                1.0 - norm_val
            }
        } else {
            norm_val
        };

        let bar_progress = if is_bipolar {
            ((effective_norm + 1.0) / 2.0).clamp(0.0, 1.0) as f32
        } else {
            effective_norm.clamp(0.0, 1.0) as f32
        };

        let bar_text = if is_bipolar {
            format!("{:+.1}% (Centered)", effective_norm * 100.0)
        } else {
            format!("{:.1}%", effective_norm * 100.0)
        };

        ui.add(
            egui::ProgressBar::new(bar_progress)
                .show_percentage()
                .text(bar_text)
                .animate(true),
        );

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Raw Evdev: {}", raw_val)).strong());
            ui.separator();
            ui.label(format!("Hardware Limits: [{} .. {}]", min_val, max_val));
            ui.separator();
            let polarity = if is_bipolar {
                "Bipolar (-Max .. +Max)"
            } else {
                "Unipolar (0 .. Max)"
            };
            ui.label(
                RichText::new(polarity)
                    .color(Color32::from_rgb(180, 200, 255))
                    .small(),
            );
        });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut state.axis_invert, "Invert Axis Direction");
            ui.separator();
            ui.add(egui::Slider::new(&mut state.axis_deadzone_pct, 0.0..=15.0).text("Deadzone %"));
        });
    });

    ui.add_space(8.0);

    // --- SimConnect Event Binding Section ---
    ui.group(|ui| {
        ui.heading(
            RichText::new(format!(
                "Bind Axis on '{}' into Current Multi-Device Profile",
                active_dev.name
            ))
            .color(Color32::from_rgb(140, 220, 140)),
        );
        ui.add_space(4.0);

        if state.selected_axis_preset_idx >= SIMCONNECT_AXIS_PRESETS.len() {
            state.selected_axis_preset_idx = 0;
        }

        let current_preset = &SIMCONNECT_AXIS_PRESETS[state.selected_axis_preset_idx];

        ui.horizontal(|ui| {
            ui.label("Axis Preset:");
            egui::ComboBox::from_id_salt("axis_preset_combo")
                .width(360.0)
                .selected_text(format!(
                    "[{}] {}",
                    current_preset.category, current_preset.description
                ))
                .show_ui(ui, |ui| {
                    for (idx, p) in SIMCONNECT_AXIS_PRESETS.iter().enumerate() {
                        ui.selectable_value(
                            &mut state.selected_axis_preset_idx,
                            idx,
                            format!("[{}] {} ({})", p.category, p.description, p.name),
                        );
                    }
                });
        });

        let current_preset = &SIMCONNECT_AXIS_PRESETS[state.selected_axis_preset_idx];
        ui.label(
            RichText::new(format!(
                "Target SimEvent: {} (Scale: {})",
                current_preset.name,
                if current_preset.is_bipolar {
                    "-16383 .. 16383"
                } else {
                    "0 .. 16383"
                }
            ))
            .italics()
            .color(Color32::LIGHT_GRAY),
        );

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            if ui
                .button(
                    RichText::new(format!(
                        "➕ Bind Axis on '{}' & Save to Profile",
                        active_dev.name
                    ))
                    .color(Color32::from_rgb(100, 255, 100))
                    .strong(),
                )
                .clicked()
            {
                add_axis_action_to_profile(profile, state, devices, current_preset.name);
                sync_and_save_profile(profile, profile_path_input, cmd_tx);
            }

            if ui.button("⚡ Test Transmit Axis Value").clicked() {
                let maybe_ax = live_axes
                    .get(&format!("{}:{}", active_dev.name, state.selected_input))
                    .or_else(|| live_axes.get(&state.selected_input));
                let norm = maybe_ax.map(|a| a.normalized).unwrap_or(0.0);
                let eff = if state.axis_invert {
                    if current_preset.is_bipolar {
                        -norm
                    } else {
                        1.0 - norm
                    }
                } else {
                    norm
                };
                let sim_val = if current_preset.is_bipolar {
                    (eff * 16383.0).round().clamp(-16383.0, 16383.0) as i32
                } else {
                    (eff * 16383.0).round().clamp(0.0, 16383.0) as i32
                };
                let _ = cmd_tx.send(GuiCommand::FireAxisEvent {
                    event_name: current_preset.name.to_string(),
                    value: sim_val,
                });
            }
        });
    });
}

fn render_button_editor(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    profile_path_input: &str,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    discovered_lvars: &[String],
    devices: &[ConnectedDevice],
) {
    let active_dev_name = devices
        .get(state.selected_device_idx)
        .map(|d| d.name.as_str())
        .unwrap_or("Selected Controller");

    ui.group(|ui| {
        ui.heading(
            RichText::new(format!(
                "Add Action to '{}' in Current Multi-Device Profile",
                active_dev_name
            ))
            .color(Color32::from_rgb(140, 220, 140)),
        );
        ui.add_space(4.0);

        // Trigger selector
        ui.horizontal(|ui| {
            ui.label("Event Trigger:");
            ui.selectable_value(
                &mut state.selected_trigger,
                "PRESS".to_string(),
                "On Press / Activate",
            );
            ui.selectable_value(
                &mut state.selected_trigger,
                "RELEASE".to_string(),
                "On Release",
            );
            ui.selectable_value(
                &mut state.selected_trigger,
                "CW".to_string(),
                "Clockwise / Increment",
            );
            ui.selectable_value(
                &mut state.selected_trigger,
                "CCW".to_string(),
                "Counter-Clockwise / Decrement",
            );
        });

        ui.add_space(4.0);

        // Action Type
        ui.horizontal(|ui| {
            ui.label("Action Type:");
            ui.selectable_value(
                &mut state.new_action_type,
                ActionTypeChoice::SimConnectPreset,
                "SimConnect Preset",
            );
            ui.selectable_value(
                &mut state.new_action_type,
                ActionTypeChoice::SimConnectCustom,
                "Custom SimEvent",
            );
            ui.selectable_value(
                &mut state.new_action_type,
                ActionTypeChoice::Lvar,
                "LVAR",
            );
            ui.selectable_value(
                &mut state.new_action_type,
                ActionTypeChoice::CalculatorCode,
                "RPN Calculator Code",
            );
        });

        ui.add_space(4.0);

        match state.new_action_type {
            ActionTypeChoice::SimConnectPreset => {
                ui.horizontal(|ui| {
                    ui.label("Preset:");
                    let current_preset = &SIMCONNECT_PRESETS[state.selected_preset_idx];
                    egui::ComboBox::from_id_salt("preset_combo")
                        .width(320.0)
                        .selected_text(format!(
                            "[{}] {}",
                            current_preset.category, current_preset.name
                        ))
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
                        .color(Color32::LIGHT_GRAY),
                );
            }
            ActionTypeChoice::SimConnectCustom => {
                ui.horizontal(|ui| {
                    ui.label("Event Name:");
                    ui.text_edit_singleline(&mut state.custom_target);
                });
                ui.horizontal(|ui| {
                    ui.label("Value / Param:");
                    ui.text_edit_singleline(&mut state.action_value);
                });
            }
            ActionTypeChoice::Lvar => {
                ui.horizontal(|ui| {
                    ui.label("Target LVAR:");
                    ui.text_edit_singleline(&mut state.custom_target);
                });
                if !discovered_lvars.is_empty() {
                    ui.collapsing("Choose from Discovered LVARs", |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for lvar in discovered_lvars {
                                    if ui.button(lvar).clicked() {
                                        state.custom_target = lvar.clone();
                                    }
                                }
                            });
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Target Value:");
                    ui.text_edit_singleline(&mut state.action_value);
                });
            }
            ActionTypeChoice::CalculatorCode => {
                ui.horizontal(|ui| {
                    ui.label("RPN Code:");
                    ui.text_edit_singleline(&mut state.custom_target);
                });
                ui.collapsing("Quick Templates", |ui| {
                    for &(code, desc) in COMMON_CALCULATOR_CODES {
                        if ui.button(format!("{}: {}", desc, code)).clicked() {
                            state.custom_target = code.to_string();
                        }
                    }
                });
            }
        }

        ui.add_space(4.0);

        // Optional Condition
        ui.checkbox(
            &mut state.use_condition,
            "Add Execution Condition (Only execute if condition is met)",
        );
        if state.use_condition {
            ui.horizontal(|ui| {
                ui.label("Variable:");
                ui.text_edit_singleline(&mut state.cond_target);
                ui.label("Condition:");
                egui::ComboBox::from_id_salt("cond_comp_combo")
                    .selected_text(&state.cond_comp)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.cond_comp, "Equals".to_string(), "==");
                        ui.selectable_value(&mut state.cond_comp, "Unequal".to_string(), "!=");
                        ui.selectable_value(&mut state.cond_comp, "Greater".to_string(), ">");
                        ui.selectable_value(&mut state.cond_comp, "Less".to_string(), "<");
                    });
                ui.label("Value:");
                ui.text_edit_singleline(&mut state.cond_value);
            });
        }

        ui.add_space(8.0);

        // Submit button
        ui.horizontal(|ui| {
            if ui
                .button(
                    RichText::new(format!(
                        "➕ Add Action to '{}' & Save Profile",
                        active_dev_name
                    ))
                    .color(Color32::from_rgb(50, 255, 120))
                    .strong(),
                )
                .clicked()
            {
                if !devices.is_empty() && !state.selected_input.is_empty() {
                    add_action_to_profile(profile, state, devices);
                    sync_and_save_profile(profile, profile_path_input, cmd_tx);
                }
            }

            if ui.button("Test Action in Simulator").clicked() {
                match state.new_action_type {
                    ActionTypeChoice::SimConnectPreset => {
                        let p = &SIMCONNECT_PRESETS[state.selected_preset_idx];
                        let _ = cmd_tx.send(GuiCommand::FireSimConnectEvent {
                            event_name: p.name.to_string(),
                            value: p.default_value,
                        });
                    }
                    ActionTypeChoice::SimConnectCustom => {
                        let val = state.action_value.parse::<u32>().unwrap_or(0);
                        let name = state
                            .custom_target
                            .strip_prefix("SIMCONNECT:")
                            .unwrap_or(&state.custom_target)
                            .to_string();
                        let _ = cmd_tx.send(GuiCommand::FireSimConnectEvent {
                            event_name: name,
                            value: val,
                        });
                    }
                    ActionTypeChoice::Lvar => {
                        let val = state.action_value.parse::<f64>().unwrap_or(1.0);
                        let name = state
                            .custom_target
                            .strip_prefix("LVAR:")
                            .unwrap_or(&state.custom_target)
                            .to_string();
                        let _ = cmd_tx.send(GuiCommand::SetLvar { name, value: val });
                    }
                    ActionTypeChoice::CalculatorCode => {
                        let _ = cmd_tx.send(GuiCommand::ExecCalculatorCode {
                            code: state.custom_target.clone(),
                        });
                    }
                }
            }
        });
    });
}

fn sync_and_save_profile(
    profile: &Option<SpadProfile>,
    profile_path_input: &str,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    if let Some(ref prof) = profile {
        let save_path = if profile_path_input.trim().is_empty() {
            PathBuf::from("profiles/custom_multi_device.xml")
        } else {
            PathBuf::from(profile_path_input.trim())
        };
        let _ = cmd_tx.send(GuiCommand::UpdateAndSaveProfile {
            profile: prof.clone(),
            path: save_path,
            bind_to_aircraft: None,
        });
    }
}

fn count_actions_for_device_input(
    profile: &Option<SpadProfile>,
    active_device: &ConnectedDevice,
    input_name: &str,
) -> usize {
    if let Some(prof) = profile {
        if let Some(dev_prof) = prof.find_device_for_connected(
            active_device.vendor_id,
            active_device.product_id,
            &active_device.id,
        ) {
            let mut total = 0;
            for page in &dev_prof.pages {
                if let Some(ref evs) = page.events {
                    for ev in &evs.items {
                        if crate::profile::schema::bound_to_matches(&ev.bound_to, input_name) {
                            if let Some(ref defs) = ev.event_definitions {
                                total += defs
                                    .items
                                    .iter()
                                    .map(|d| d.actions.as_ref().map(|a| a.items.len()).unwrap_or(0))
                                    .sum::<usize>();
                            }
                        }
                    }
                }
            }
            return total;
        }
    }
    0
}

fn add_action_to_profile(
    profile: &mut Option<SpadProfile>,
    state: &MappingViewState,
    devices: &[ConnectedDevice],
) {
    if devices.is_empty() || state.selected_device_idx >= devices.len() {
        return;
    }
    let active_dev = &devices[state.selected_device_idx];

    if profile.is_none() {
        *profile = Some(SpadProfile {
            version: Some(2),
            is_stock_profile: Some(false),
            name: Some("Custom Multi-Device Profile".to_string()),
            variables: None,
            options: None,
            assigned_aircrafts: None,
            devices: Some(crate::profile::schema::DeviceList { items: Vec::new() }),
            profile_info: None,
        });
    }

    let prof = profile.as_mut().unwrap();
    let dev_path_str = active_dev
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    let dev = prof.ensure_device_for_connected(
        active_dev.vendor_id,
        active_dev.product_id,
        &active_dev.id,
        &active_dev.name,
        dev_path_str.as_deref(),
    );

    if dev.pages.is_empty() {
        dev.pages.push(crate::profile::schema::DevicePage {
            id: Some("Default".to_string()),
            is_default: Some(true),
            name: Some("Default".to_string()),
            events: Some(EventList { items: Vec::new() }),
        });
    }

    let page = &mut dev.pages[0];
    if page.events.is_none() {
        page.events = Some(EventList { items: Vec::new() });
    }

    let events = page.events.as_mut().unwrap();
    let ev_idx = match events
        .items
        .iter()
        .position(|e| crate::profile::schema::bound_to_matches(&e.bound_to, &state.selected_input))
    {
        Some(idx) => idx,
        None => {
            events.items.push(SpadEvent {
                bound_to: state.selected_input.clone(),
                config_id: None,
                event_definitions: Some(EventDefinitionsList { items: Vec::new() }),
            });
            events.items.len() - 1
        }
    };

    let spad_event = &mut events.items[ev_idx];
    if spad_event.event_definitions.is_none() {
        spad_event.event_definitions = Some(EventDefinitionsList { items: Vec::new() });
    }

    let defs = spad_event.event_definitions.as_mut().unwrap();

    // Create action
    let new_action = match state.new_action_type {
        ActionTypeChoice::SimConnectPreset => {
            let p = &SIMCONNECT_PRESETS[state.selected_preset_idx];
            EventActionItem::Control(EventActionControl {
                target: format!("SIMCONNECT:{}", p.name),
                value: Some(p.default_value.to_string()),
            })
        }
        ActionTypeChoice::SimConnectCustom => {
            let t = if state.custom_target.starts_with("SIMCONNECT:") {
                state.custom_target.clone()
            } else {
                format!("SIMCONNECT:{}", state.custom_target)
            };
            EventActionItem::Control(EventActionControl {
                target: t,
                value: Some(state.action_value.clone()),
            })
        }
        ActionTypeChoice::Lvar => {
            let t = if state.custom_target.starts_with("LVAR:") {
                state.custom_target.clone()
            } else {
                format!("LVAR:{}", state.custom_target)
            };
            EventActionItem::ChangeValue(crate::profile::schema::EventActionChangeValue {
                target: t,
                value: Some(state.action_value.clone()),
                operation: Some("Set".to_string()),
            })
        }
        ActionTypeChoice::CalculatorCode => EventActionItem::Script(EventActionScript {
            script: Some(state.custom_target.clone()),
            language: Some("RPN".to_string()),
        }),
    };

    // Create condition if enabled
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
        priority: Some(0),
        is_final: Some(false),
        condition_binding: Some("All".to_string()),
        conditions,
        actions: Some(crate::profile::schema::EventActionsList {
            items: vec![new_action],
        }),
    });
}

fn remove_action_from_device_in_profile(
    profile: &mut Option<SpadProfile>,
    active_dev: &ConnectedDevice,
    bound_to: &str,
    def_idx: usize,
    act_idx: usize,
) {
    if let Some(ref mut prof) = profile {
        if let Some(ref mut devs) = prof.devices {
            for dev in &mut devs.items {
                if !dev.matches_vid_pid(active_dev.vendor_id, active_dev.product_id)
                    && !dev.matches_device_id(&active_dev.id)
                {
                    continue;
                }
                for page in &mut dev.pages {
                    if let Some(ref mut evs) = page.events {
                        for ev in &mut evs.items {
                            if crate::profile::schema::bound_to_matches(&ev.bound_to, bound_to) {
                                if let Some(ref mut defs) = ev.event_definitions {
                                    if def_idx < defs.items.len() {
                                        if let Some(ref mut acts) = defs.items[def_idx].actions {
                                            if act_idx < acts.items.len() {
                                                acts.items.remove(act_idx);
                                            }
                                        }
                                        if defs.items[def_idx]
                                            .actions
                                            .as_ref()
                                            .map(|a| a.items.is_empty())
                                            .unwrap_or(true)
                                        {
                                            defs.items.remove(def_idx);
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
}

fn add_axis_action_to_profile(
    profile: &mut Option<SpadProfile>,
    state: &MappingViewState,
    devices: &[ConnectedDevice],
    event_name: &str,
) {
    if devices.is_empty() || state.selected_device_idx >= devices.len() {
        return;
    }
    let active_dev = &devices[state.selected_device_idx];

    if profile.is_none() {
        *profile = Some(SpadProfile {
            version: Some(2),
            is_stock_profile: Some(false),
            name: Some("Custom Multi-Device Profile".to_string()),
            variables: None,
            options: None,
            assigned_aircrafts: None,
            devices: Some(crate::profile::schema::DeviceList { items: Vec::new() }),
            profile_info: None,
        });
    }

    let prof = profile.as_mut().unwrap();
    let dev_path_str = active_dev
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    let dev = prof.ensure_device_for_connected(
        active_dev.vendor_id,
        active_dev.product_id,
        &active_dev.id,
        &active_dev.name,
        dev_path_str.as_deref(),
    );

    if dev.pages.is_empty() {
        dev.pages.push(crate::profile::schema::DevicePage {
            id: Some("Default".to_string()),
            is_default: Some(true),
            name: Some("Default".to_string()),
            events: Some(EventList { items: Vec::new() }),
        });
    }

    let page = &mut dev.pages[0];
    if page.events.is_none() {
        page.events = Some(EventList { items: Vec::new() });
    }

    let events = page.events.as_mut().unwrap();
    let ev_idx = match events
        .items
        .iter()
        .position(|e| crate::profile::schema::bound_to_matches(&e.bound_to, &state.selected_input))
    {
        Some(idx) => idx,
        None => {
            events.items.push(SpadEvent {
                bound_to: state.selected_input.clone(),
                config_id: None,
                event_definitions: Some(EventDefinitionsList { items: Vec::new() }),
            });
            events.items.len() - 1
        }
    };

    let spad_event = &mut events.items[ev_idx];
    if spad_event.event_definitions.is_none() {
        spad_event.event_definitions = Some(EventDefinitionsList { items: Vec::new() });
    }

    let defs = spad_event.event_definitions.as_mut().unwrap();

    let new_action = EventActionItem::Control(EventActionControl {
        target: format!("SIMCONNECT:{}", event_name),
        value: Some("AXIS_VALUE".to_string()),
    });

    let new_def = EventDefinition {
        trigger: Some("MOVE".to_string()),
        priority: Some(0),
        is_final: Some(false),
        condition_binding: Some("All".to_string()),
        conditions: None,
        actions: Some(crate::profile::schema::EventActionsList {
            items: vec![new_action],
        }),
    };

    defs.items.push(new_def);
}
