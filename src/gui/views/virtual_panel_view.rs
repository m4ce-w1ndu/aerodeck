use eframe::egui::{self, Color32, RichText, Ui, Vec2};

use crate::gui::backend::GuiCommand;

pub struct VirtualPanelViewState {
    // Switch states for UI simulation
    pub bat_on: bool,
    pub alt_on: bool,
    pub avionics_on: bool,
    pub pump_on: bool,
    pub pitot_on: bool,
    pub deice_on: bool,
    pub gear_down: bool,
    pub lights_nav: bool,
    pub lights_beacon: bool,
    pub lights_strobe: bool,
    pub lights_taxi: bool,
    pub lights_landing: bool,
    pub mag_index: usize, // 0=OFF, 1=R, 2=L, 3=BOTH, 4=START

    // Multi panel states
    pub ap_active: bool,
    pub hdg_active: bool,
    pub nav_active: bool,
    pub alt_active: bool,
    pub vs_active: bool,
    pub simulated_alt: u32,
    pub simulated_vs: i32,
    pub simulated_hdg: u32,
}

impl Default for VirtualPanelViewState {
    fn default() -> Self {
        Self {
            bat_on: false,
            alt_on: false,
            avionics_on: false,
            pump_on: false,
            pitot_on: false,
            deice_on: false,
            gear_down: true,
            lights_nav: false,
            lights_beacon: false,
            lights_strobe: false,
            lights_taxi: false,
            lights_landing: false,
            mag_index: 0,
            ap_active: false,
            hdg_active: false,
            nav_active: false,
            alt_active: false,
            vs_active: false,
            simulated_alt: 5500,
            simulated_vs: 0,
            simulated_hdg: 360,
        }
    }
}

pub fn render_virtual_panel_view(
    ui: &mut Ui,
    state: &mut VirtualPanelViewState,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    ui.heading(RichText::new("Interactive Virtual Flight Panel Simulation").color(Color32::from_rgb(255, 180, 50)));
    ui.label("Click on switches, buttons, and dials below to trigger controller events and test your mapped profile actions live.");
    ui.add_space(8.0);

    // 1. SWITCH PANEL SIMULATION
    ui.group(|ui| {
        ui.heading("Saitek Pro Flight Switch Panel");
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            // Master switches
            render_toggle_switch(ui, &mut state.bat_on, "BATTERY", "SWITCH_BATTERY", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.alt_on, "ALTERNATOR", "SWITCH_ALTERNATOR", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.avionics_on, "AVIONICS", "SWITCH_AVIONICS", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.pump_on, "FUEL PUMP", "SWITCH_FUEL_PUMP", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.deice_on, "DE-ICE", "SWITCH_DEICE", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.pitot_on, "PITOT HEAT", "SWITCH_PITOT_HEAT", cmd_tx, "switch_panel");

            ui.separator();

            // Lights
            render_toggle_switch(ui, &mut state.lights_beacon, "BEACON", "SWITCH_LIGHTS_BEACON", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.lights_nav, "NAV", "SWITCH_LIGHTS_NAV", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.lights_strobe, "STROBE", "SWITCH_LIGHTS_STROBE", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.lights_taxi, "TAXI", "SWITCH_LIGHTS_TAXI", cmd_tx, "switch_panel");
            render_toggle_switch(ui, &mut state.lights_landing, "LANDING", "SWITCH_LIGHTS_LANDING", cmd_tx, "switch_panel");

            ui.separator();

            // Gear Lever
            let gear_label = if state.gear_down { "GEAR: DOWN" } else { "GEAR: UP" };
            let gear_color = if state.gear_down { Color32::from_rgb(0, 255, 50) } else { Color32::from_rgb(255, 60, 60) };
            if ui.button(RichText::new(gear_label).color(gear_color).strong()).clicked() {
                state.gear_down = !state.gear_down;
                let bound = if state.gear_down { "GEAR_LEVER_DOWN" } else { "GEAR_LEVER_UP" };
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "switch_panel".to_string(),
                    bound_to: bound.to_string(),
                    action: "press".to_string(),
                });
            }
        });

        ui.add_space(4.0);

        // Magnetos rotary
        ui.horizontal(|ui| {
            ui.label("Magnetos Rotary Switch:");
            let mag_labels = ["OFF", "RIGHT", "LEFT", "BOTH", "START"];
            let mag_bounds = ["SWITCH_MAG_OFF", "SWITCH_MAG_RIGHT", "SWITCH_MAG_LEFT", "SWITCH_MAG_BOTH", "SWITCH_MAG_START"];

            for (idx, &label) in mag_labels.iter().enumerate() {
                let is_sel = state.mag_index == idx;
                if ui.selectable_label(is_sel, label).clicked() {
                    state.mag_index = idx;
                    let _ = cmd_tx.send(GuiCommand::SimulateInput {
                        device: "switch_panel".to_string(),
                        bound_to: mag_bounds[idx].to_string(),
                        action: "press".to_string(),
                    });
                }
            }
        });
    });

    ui.add_space(12.0);

    // 2. MULTI PANEL SIMULATION
    ui.group(|ui| {
        ui.heading("Saitek Pro Flight Multi Panel (Autopilot & Trim)");
        ui.add_space(4.0);

        // 7-segment digital display simulation
        egui::Frame::none()
            .fill(Color32::from_rgb(10, 20, 10))
            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 200, 50)))
            .rounding(4.0)
            .inner_margin(Vec2::new(12.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("ALT: {:05} FT", state.simulated_alt)).color(Color32::from_rgb(0, 255, 80)).monospace().size(18.0));
                    ui.add_space(20.0);
                    ui.label(RichText::new(format!("VS: {:+05} FPM", state.simulated_vs)).color(Color32::from_rgb(0, 255, 80)).monospace().size(18.0));
                    ui.add_space(20.0);
                    ui.label(RichText::new(format!("HDG: {:03}°", state.simulated_hdg)).color(Color32::from_rgb(0, 255, 80)).monospace().size(18.0));
                });
            });

        ui.add_space(6.0);

        // Autopilot buttons
        ui.horizontal(|ui| {
            render_ap_button(ui, &mut state.ap_active, "AP", "AP_BUTTON_AP", cmd_tx);
            render_ap_button(ui, &mut state.hdg_active, "HDG", "AP_BUTTON_HDG", cmd_tx);
            render_ap_button(ui, &mut state.nav_active, "NAV", "AP_BUTTON_NAV", cmd_tx);
            render_ap_button(ui, &mut state.alt_active, "ALT", "AP_BUTTON_ALT", cmd_tx);
            render_ap_button(ui, &mut state.vs_active, "VS", "AP_BUTTON_VS", cmd_tx);

            ui.separator();

            // Rotary knob simulation
            ui.label("Value Rotary Dial:");
            if ui.button("◀ DEC (CCW)").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "multi_panel".to_string(),
                    bound_to: "AP_ROTARY_CCW".to_string(),
                    action: "ccw".to_string(),
                });
            }
            if ui.button("INC (CW) ▶").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "multi_panel".to_string(),
                    bound_to: "AP_ROTARY_CW".to_string(),
                    action: "cw".to_string(),
                });
            }

            ui.separator();

            // Trim wheel & Flaps
            ui.label("Trim Wheel:");
            if ui.button("⏶ NOSE UP").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "multi_panel".to_string(),
                    bound_to: "PITCH_TRIM_UP".to_string(),
                    action: "press".to_string(),
                });
            }
            if ui.button("⏷ NOSE DN").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "multi_panel".to_string(),
                    bound_to: "PITCH_TRIM_DOWN".to_string(),
                    action: "press".to_string(),
                });
            }
        });
    });

    ui.add_space(12.0);

    // 3. RADIO PANEL SIMULATION
    ui.group(|ui| {
        ui.heading("Saitek Pro Flight Radio Panel");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("ACT / STBY SWAP").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "radio_panel".to_string(),
                    bound_to: "RADIO_ACT_STBY".to_string(),
                    action: "press".to_string(),
                });
            }

            ui.separator();

            ui.label("Coarse Dial:");
            if ui.button("◀ -1 MHz").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "radio_panel".to_string(),
                    bound_to: "RADIO_COARSE_CCW".to_string(),
                    action: "ccw".to_string(),
                });
            }
            if ui.button("+1 MHz ▶").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "radio_panel".to_string(),
                    bound_to: "RADIO_COARSE_CW".to_string(),
                    action: "cw".to_string(),
                });
            }

            ui.separator();

            ui.label("Fine Dial:");
            if ui.button("◀ -25 kHz").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "radio_panel".to_string(),
                    bound_to: "RADIO_FINE_CCW".to_string(),
                    action: "ccw".to_string(),
                });
            }
            if ui.button("+25 kHz ▶").clicked() {
                let _ = cmd_tx.send(GuiCommand::SimulateInput {
                    device: "radio_panel".to_string(),
                    bound_to: "RADIO_FINE_CW".to_string(),
                    action: "cw".to_string(),
                });
            }
        });
    });
}

fn render_toggle_switch(
    ui: &mut Ui,
    state: &mut bool,
    label: &str,
    bound_to: &str,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
    device: &str,
) {
    let text = if *state { format!("⏺ {}", label) } else { format!("○ {}", label) };
    let color = if *state { Color32::from_rgb(0, 255, 120) } else { Color32::GRAY };

    if ui.button(RichText::new(text).color(color)).clicked() {
        *state = !*state;
        let action = if *state { "press" } else { "release" };
        let _ = cmd_tx.send(GuiCommand::SimulateInput {
            device: device.to_string(),
            bound_to: bound_to.to_string(),
            action: action.to_string(),
        });
    }
}

fn render_ap_button(
    ui: &mut Ui,
    active: &mut bool,
    label: &str,
    bound_to: &str,
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<GuiCommand>,
) {
    let color = if *active { Color32::from_rgb(0, 255, 80) } else { Color32::WHITE };
    if ui.button(RichText::new(label).color(color).strong()).clicked() {
        *active = !*active;
        let _ = cmd_tx.send(GuiCommand::SimulateInput {
            device: "multi_panel".to_string(),
            bound_to: bound_to.to_string(),
            action: "press".to_string(),
        });
    }
}
