use eframe::egui::{self, Color32, RichText, Ui, Vec2};
use std::time::Instant;

use crate::gui::backend::GuiCommand;
use crate::gui::presets::{COMMON_CALCULATOR_CODES, SIMCONNECT_PRESETS};
use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::schema::{
    EventActionControl, EventActionItem, EventActionScript, EventConditionItem, EventConditionSimple,
    EventConditionsList, EventDefinition, EventDefinitionsList, EventList, SpadEvent, SpadProfile,
};

pub struct MappingViewState {
    pub selected_device_idx: usize,
    pub selected_input: String,
    pub filter_query: String,
    pub follow_active_input: bool,
    pub input_type_filter: InputFilterType,

    // Add action form state
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

pub fn render_mapping_view(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    last_input: &Option<(String, String, String, Instant)>,
    discovered_lvars: &[String],
    devices: &[ConnectedDevice],
) {
    // 1. TOP BANNER: Live Real Hardware Input Detection
    render_live_input_banner(ui, state, last_input, devices);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // 2. MAIN 2-COLUMN SPLIT: Left = Device & Input List, Right = Mapping & Action Editor
    ui.columns(2, |columns| {
        // --- LEFT COLUMN: Connected Hardware Device selector & Inputs ---
        render_device_and_input_column(&mut columns[0], state, profile, last_input, devices, cmd_tx);

        // --- RIGHT COLUMN: Mappings & Action Configurator ---
        render_action_editor_column(&mut columns[1], state, profile, cmd_tx, discovered_lvars, devices);
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
                            .color(if is_recent { Color32::from_rgb(100, 255, 100) } else { Color32::LIGHT_GRAY })
                            .strong(),
                    );
                });

            // Follow active input if enabled
            if state.follow_active_input && is_recent && elapsed_ms < 200 {
                for (idx, d) in devices.iter().enumerate() {
                    if d.name == dev.as_str() || d.id == dev.as_str() || d.inputs.iter().any(|i| i == bound) {
                        state.selected_device_idx = idx;
                        state.selected_input = bound.clone();
                        break;
                    }
                }
            }
        } else {
            ui.label(RichText::new("Awaiting controller input... Move a switch or turn a knob on your physical hardware").italics().color(Color32::GRAY));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(&mut state.follow_active_input, "Auto-Select Active Hardware Input");
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
) {
    ui.heading(RichText::new("Connected Flight Hardware & Inputs").color(Color32::from_rgb(100, 180, 255)));
    ui.add_space(4.0);

    if devices.is_empty() {
        ui.label(RichText::new("No controllers detected. Plug in your joystick, throttle, or yoke and click Rescan.").color(Color32::LIGHT_RED));
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
    if state.selected_input.is_empty() && !active_device.inputs.is_empty() {
        state.selected_input = active_device.inputs[0].clone();
    }

    // Device selector dropdown & Rescan button
    ui.horizontal(|ui| {
        ui.label("Active Device:");
        let selected_text = format!("{} [{}]", active_device.name, active_device.category.badge());
        egui::ComboBox::from_id_salt("connected_device_combo")
            .width(280.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (idx, dev) in devices.iter().enumerate() {
                    let label = format!("{} [{}]", dev.name, dev.category.badge());
                    if ui.selectable_value(&mut state.selected_device_idx, idx, label).clicked() {
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
                ui.label(RichText::new(format!("VID: {:04X}  PID: {:04X}", active_device.vendor_id, active_device.product_id)).color(Color32::LIGHT_GRAY).small());
                ui.separator();
                ui.label(RichText::new(format!("{} Buttons", active_device.buttons.len())).color(Color32::from_rgb(0, 220, 150)).small());
                ui.separator();
                ui.label(RichText::new(format!("{} Axes", active_device.axes.len())).color(Color32::from_rgb(0, 200, 255)).small());
                if let Some(ref p) = active_device.path {
                    ui.separator();
                    ui.label(RichText::new(format!("{:?}", p)).color(Color32::GRAY).small());
                }
            });
        });

    ui.add_space(6.0);

    // Filter controls: Type filter (All / Buttons / Axes) + Search box
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.input_type_filter, InputFilterType::All, "All");
        ui.selectable_value(&mut state.input_type_filter, InputFilterType::Buttons, format!("Buttons ({})", active_device.buttons.len()));
        ui.selectable_value(&mut state.input_type_filter, InputFilterType::Axes, format!("Axes ({})", active_device.axes.len()));

        ui.separator();

        ui.label("Search:");
        ui.add(egui::TextEdit::singleline(&mut state.filter_query).desired_width(100.0));
        if ui.button("✖").clicked() {
            state.filter_query.clear();
        }
    });

    ui.add_space(4.0);

    let inputs_to_display: Vec<&String> = match state.input_type_filter {
        InputFilterType::All => active_device.inputs.iter().collect(),
        InputFilterType::Buttons => active_device.buttons.iter().collect(),
        InputFilterType::Axes => active_device.axes.iter().collect(),
    };

    // Scrollable input list
    egui::ScrollArea::vertical()
        .max_height(460.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            for inp in inputs_to_display {
                if !state.filter_query.is_empty()
                    && !inp.to_lowercase().contains(&state.filter_query.to_lowercase())
                {
                    continue;
                }

                let is_selected = &state.selected_input == inp;
                let is_last_active = last_input
                    .as_ref()
                    .map(|(_, b, _, inst)| b == inp && inst.elapsed().as_millis() < 2000)
                    .unwrap_or(false);

                // Count mapped actions in profile
                let mapped_count = count_actions_for_input(profile, inp);

                let label_text = if mapped_count > 0 {
                    format!("{}  [{} action{}]", inp, mapped_count, if mapped_count == 1 { "" } else { "s" })
                } else {
                    inp.to_string()
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

fn render_action_editor_column(
    ui: &mut Ui,
    state: &mut MappingViewState,
    profile: &mut Option<SpadProfile>,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    discovered_lvars: &[String],
    devices: &[ConnectedDevice],
) {
    let display_input = if state.selected_input.is_empty() {
        "Select an input on the left"
    } else {
        &state.selected_input
    };

    ui.heading(RichText::new(format!("Mapping: {}", display_input)).color(Color32::from_rgb(255, 200, 100)));
    ui.add_space(4.0);

    // 1. Current bindings list
    ui.group(|ui| {
        ui.label(RichText::new("Configured Profile Actions:").strong());
        ui.add_space(2.0);

        let mut to_remove: Option<(usize, usize)> = None;
        let mut to_test: Option<GuiCommand> = None;

        if let Some(prof) = profile {
            if let Some(ref mut devs) = prof.devices {
                for dev in &mut devs.items {
                    for page in &mut dev.pages {
                        if let Some(ref mut evs) = page.events {
                            for ev in &mut evs.items {
                                if ev.bound_to == state.selected_input {
                                    if let Some(ref mut defs) = ev.event_definitions {
                                        for (def_idx, def) in defs.items.iter().enumerate() {
                                            let trigger = def.trigger.as_deref().unwrap_or("ALL");
                                            ui.label(RichText::new(format!("Trigger: [{}]", trigger)).color(Color32::LIGHT_BLUE));

                                            if let Some(ref acts) = def.actions {
                                                for (act_idx, act) in acts.items.iter().enumerate() {
                                                    ui.horizontal(|ui| {
                                                        match act {
                                                            EventActionItem::Control(c) => {
                                                                ui.label(format!("⚙ Control: {} (val: {:?})", c.target, c.value));
                                                                if ui.button("Test").clicked() {
                                                                    let val = c.value.as_deref().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                                                                    let name = c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string();
                                                                    to_test = Some(GuiCommand::FireSimConnectEvent { event_name: name, value: val });
                                                                }
                                                            }
                                                            EventActionItem::ChangeValue(cv) => {
                                                                ui.label(format!("📝 LVAR: {} = {:?}", cv.target, cv.value));
                                                                if ui.button("Test").clicked() {
                                                                    let val = cv.value.as_deref().and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0);
                                                                    let name = cv.target.strip_prefix("LVAR:").unwrap_or(&cv.target).to_string();
                                                                    to_test = Some(GuiCommand::SetLvar { name, value: val });
                                                                }
                                                            }
                                                            EventActionItem::Script(sc) => {
                                                                let script_text = sc.script.clone().unwrap_or_default();
                                                                ui.label(format!("💻 RPN: {}", script_text));
                                                                if ui.button("Test").clicked() {
                                                                    to_test = Some(GuiCommand::ExecCalculatorCode { code: script_text });
                                                                }
                                                            }
                                                            _ => {
                                                                ui.label("Other action");
                                                            }
                                                        }

                                                        if ui.button(RichText::new("Delete").color(Color32::LIGHT_RED)).clicked() {
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
        } else {
            ui.label(RichText::new("No profile loaded. Use 'Load Profile' or 'New Profile' from top toolbar.").italics());
        }

        if let Some(cmd) = to_test {
            let _ = cmd_tx.send(cmd);
        }

        // Apply deletion if requested
        if let Some((def_idx, act_idx)) = to_remove {
            remove_action_from_profile(profile, &state.selected_input, def_idx, act_idx);
        }
    });

    ui.add_space(8.0);

    // 2. Add New Action Form
    ui.group(|ui| {
        ui.heading(RichText::new("Add Action / Binding").color(Color32::from_rgb(140, 220, 140)));
        ui.add_space(4.0);

        // Trigger selector
        ui.horizontal(|ui| {
            ui.label("Event Trigger:");
            ui.selectable_value(&mut state.selected_trigger, "PRESS".to_string(), "On Press / Activate");
            ui.selectable_value(&mut state.selected_trigger, "RELEASE".to_string(), "On Release");
            ui.selectable_value(&mut state.selected_trigger, "CW".to_string(), "Clockwise / Increment");
            ui.selectable_value(&mut state.selected_trigger, "CCW".to_string(), "Counter-Clockwise / Decrement");
        });

        ui.add_space(4.0);

        // Action Type
        ui.horizontal(|ui| {
            ui.label("Action Type:");
            ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::SimConnectPreset, "SimConnect Preset");
            ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::SimConnectCustom, "Custom SimEvent");
            ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::Lvar, "LVAR");
            ui.selectable_value(&mut state.new_action_type, ActionTypeChoice::CalculatorCode, "RPN Calculator Code");
        });

        ui.add_space(4.0);

        match state.new_action_type {
            ActionTypeChoice::SimConnectPreset => {
                ui.horizontal(|ui| {
                    ui.label("Preset:");
                    let current_preset = &SIMCONNECT_PRESETS[state.selected_preset_idx];
                    egui::ComboBox::from_id_salt("preset_combo")
                        .width(320.0)
                        .selected_text(format!("[{}] {}", current_preset.category, current_preset.name))
                        .show_ui(ui, |ui| {
                            for (idx, p) in SIMCONNECT_PRESETS.iter().enumerate() {
                                ui.selectable_value(&mut state.selected_preset_idx, idx, format!("[{}] {} - {}", p.category, p.name, p.description));
                            }
                        });
                });
                ui.label(RichText::new(SIMCONNECT_PRESETS[state.selected_preset_idx].description).italics().color(Color32::LIGHT_GRAY));
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
                        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
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
        ui.checkbox(&mut state.use_condition, "Add Execution Condition (Only execute if condition is met)");
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
            if ui.button(RichText::new("➕ Add Action to Selected Input").color(Color32::from_rgb(50, 255, 120)).strong()).clicked() {
                if !devices.is_empty() && !state.selected_input.is_empty() {
                    add_action_to_profile(profile, state, devices);
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
                        let name = state.custom_target.strip_prefix("SIMCONNECT:").unwrap_or(&state.custom_target).to_string();
                        let _ = cmd_tx.send(GuiCommand::FireSimConnectEvent { event_name: name, value: val });
                    }
                    ActionTypeChoice::Lvar => {
                        let val = state.action_value.parse::<f64>().unwrap_or(1.0);
                        let name = state.custom_target.strip_prefix("LVAR:").unwrap_or(&state.custom_target).to_string();
                        let _ = cmd_tx.send(GuiCommand::SetLvar { name, value: val });
                    }
                    ActionTypeChoice::CalculatorCode => {
                        let _ = cmd_tx.send(GuiCommand::ExecCalculatorCode { code: state.custom_target.clone() });
                    }
                }
            }
        });
    });
}

fn count_actions_for_input(profile: &Option<SpadProfile>, input_name: &str) -> usize {
    if let Some(prof) = profile {
        if let Some(ref devs) = prof.devices {
            for dev in &devs.items {
                for page in &dev.pages {
                    if let Some(ref evs) = page.events {
                        for ev in &evs.items {
                            if ev.bound_to == input_name {
                                if let Some(ref defs) = ev.event_definitions {
                                    return defs.items.iter().map(|d| d.actions.as_ref().map(|a| a.items.len()).unwrap_or(0)).sum();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    0
}

fn add_action_to_profile(profile: &mut Option<SpadProfile>, state: &MappingViewState, devices: &[ConnectedDevice]) {
    if devices.is_empty() || state.selected_device_idx >= devices.len() {
        return;
    }
    let active_dev = &devices[state.selected_device_idx];

    if profile.is_none() {
        *profile = Some(SpadProfile {
            version: Some(1),
            is_stock_profile: Some(false),
            name: Some("Custom Profile".to_string()),
            variables: None,
            options: None,
            devices: Some(crate::profile::schema::DeviceList { items: Vec::new() }),
        });
    }

    let prof = profile.as_mut().unwrap();
    if prof.devices.is_none() {
        prof.devices = Some(crate::profile::schema::DeviceList { items: Vec::new() });
    }

    let devs = prof.devices.as_mut().unwrap();

    // Find or create device profile matching active device
    let dev_idx = match devs.items.iter().position(|d| {
        d.id.as_deref() == Some(&active_dev.id)
            || d.product_id.as_deref() == Some(&format!("{:04X}", active_dev.product_id))
    }) {
        Some(idx) => idx,
        None => {
            devs.items.push(crate::profile::schema::DeviceProfile {
                vendor_id: Some(format!("{:04X}", active_dev.vendor_id)),
                product_id: Some(format!("{:04X}", active_dev.product_id)),
                device_path: active_dev.path.as_ref().map(|p| p.to_string_lossy().to_string()),
                serial: None,
                device_index: Some(0),
                name: Some(active_dev.name.clone()),
                id: Some(active_dev.id.clone()),
                pages: vec![crate::profile::schema::DevicePage {
                    id: Some("Default".to_string()),
                    is_default: Some(true),
                    name: Some("Default".to_string()),
                    events: Some(EventList { items: Vec::new() }),
                }],
            });
            devs.items.len() - 1
        }
    };

    let dev = &mut devs.items[dev_idx];
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
    let ev_idx = match events.items.iter().position(|e| e.bound_to == state.selected_input) {
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
        ActionTypeChoice::CalculatorCode => {
            EventActionItem::Script(EventActionScript {
                script: Some(state.custom_target.clone()),
                language: Some("RPN".to_string()),
            })
        }
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

fn remove_action_from_profile(profile: &mut Option<SpadProfile>, bound_to: &str, def_idx: usize, act_idx: usize) {
    if let Some(ref mut prof) = profile {
        if let Some(ref mut devs) = prof.devices {
            for dev in &mut devs.items {
                for page in &mut dev.pages {
                    if let Some(ref mut evs) = page.events {
                        for ev in &mut evs.items {
                            if ev.bound_to == bound_to {
                                if let Some(ref mut defs) = ev.event_definitions {
                                    if def_idx < defs.items.len() {
                                        if let Some(ref mut acts) = defs.items[def_idx].actions {
                                            if act_idx < acts.items.len() {
                                                acts.items.remove(act_idx);
                                            }
                                        }
                                        if defs.items[def_idx].actions.as_ref().map(|a| a.items.is_empty()).unwrap_or(true) {
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
