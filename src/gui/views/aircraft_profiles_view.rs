use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::gui::backend::GuiCommand;
use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::{
    AircraftAssignmentManager, DiscoveredAircraft, OnlineSpadProfileEntry, ProfileRepository,
    RepositoryProfileEntry, SpadAccountConfig, WindowsSpadProfileEntry, EXTRACTED_USB_DEVICES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AircraftFilterMode {
    All,
    BoundOnly,
    UnboundOnly,
    CommunityOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySourceTab {
    OnlineSpadRepository,
    WindowsSpadBackup,
    BuiltinVerified,
}

pub struct AircraftProfilesViewState {
    pub aircraft_search: String,
    pub aircraft_filter_mode: AircraftFilterMode,
    pub selected_aircraft_title: Option<String>,
    pub selected_profile_to_bind: String,

    // Right pane source tab
    pub active_repo_tab: RepositorySourceTab,

    // SPAD.neXt Account & Automatic X-Token config
    pub spad_account: SpadAccountConfig,
    pub show_account_editor: bool,
    pub is_fetching_online: bool,

    // Online SPAD.neXt catalog & filters
    pub online_catalog: Vec<OnlineSpadProfileEntry>,
    pub online_search: String,
    pub online_device_filter: String,
    pub online_match_my_devices: bool,

    // Windows SPAD.neXt profiles discovered on disk
    pub windows_profiles: Vec<WindowsSpadProfileEntry>,

    // Built-in Repository filter state
    pub match_connected_hardware_only: bool,
    pub repo_device_filter: String,
    pub repo_category_filter: String,
    pub repo_search: String,

    // Direct URL state
    pub direct_url_input: String,
    pub direct_url_save_name: String,
    pub online_soap_response: String,
    pub status_banner: Option<(String, bool)>,
}

impl Default for AircraftProfilesViewState {
    fn default() -> Self {
        let spad_account = SpadAccountConfig::load_or_autodiscover();
        let windows_profiles = ProfileRepository::discover_windows_spad_profiles(Some(
            &spad_account.windows_spad_dir,
        ));
        Self {
            aircraft_search: String::new(),
            aircraft_filter_mode: AircraftFilterMode::All,
            selected_aircraft_title: None,
            selected_profile_to_bind: "profiles/Fenix A320.xml".to_string(),
            active_repo_tab: RepositorySourceTab::OnlineSpadRepository,
            spad_account,
            show_account_editor: false,
            is_fetching_online: true,
            online_catalog: Vec::new(),
            online_search: String::new(),
            online_device_filter: "All".to_string(),
            online_match_my_devices: false,
            windows_profiles,
            match_connected_hardware_only: true,
            repo_device_filter: "All".to_string(),
            repo_category_filter: "All".to_string(),
            repo_search: String::new(),
            direct_url_input: String::new(),
            direct_url_save_name: "downloaded_profile.xml".to_string(),
            online_soap_response: String::new(),
            status_banner: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_aircraft_profiles_view(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    aircrafts: &[DiscoveredAircraft],
    assignments: &AircraftAssignmentManager,
    devices: &[ConnectedDevice],
    active_aircraft: &str,
    active_profile_name: &str,
    active_profile_path: &str,
    custom_prefix: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
) {
    let available_profiles = ProfileRepository::list_local_profiles();
    let repo_catalog = ProfileRepository::builtin_catalog();

    // Top Summary & Automatic Switching Control Card
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(24, 28, 38))
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(
                    RichText::new("✈ Aircraft-to-Profile Binding & Profile Repository")
                        .color(Color32::from_rgb(0, 225, 255))
                        .strong(),
                );
                ui.separator();

                let mut auto_switch = assignments.auto_switch_enabled;
                if ui
                    .checkbox(
                        &mut auto_switch,
                        RichText::new("⚡ Auto-Switch Profile When Aircraft Changes in MSFS")
                            .color(Color32::from_rgb(0, 255, 140))
                            .strong(),
                    )
                    .changed()
                {
                    let _ = cmd_tx.send(GuiCommand::SetAutoSwitchProfile(auto_switch));
                }

                ui.separator();
                ui.label(RichText::new("Default Fallback Profile:").color(Color32::LIGHT_GRAY));
                let current_def = assignments
                    .default_profile
                    .clone()
                    .unwrap_or_else(|| "None".to_string());
                egui::ComboBox::from_id_salt("default_fallback_profile_combo")
                    .selected_text(&current_def)
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(assignments.default_profile.is_none(), "None")
                            .clicked()
                        {
                            let _ = cmd_tx.send(GuiCommand::SetDefaultProfile(None));
                        }
                        for prof_path in &available_profiles {
                            let selected =
                                assignments.default_profile.as_deref() == Some(prof_path.as_str());
                            if ui.selectable_label(selected, prof_path).clicked() {
                                let _ = cmd_tx
                                    .send(GuiCommand::SetDefaultProfile(Some(prof_path.clone())));
                            }
                        }
                    });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔄 Rescan MSFS Aircraft").clicked() {
                        let pfx = if custom_prefix.trim().is_empty() {
                            None
                        } else {
                            Some(PathBuf::from(custom_prefix.trim()))
                        };
                        let _ = cmd_tx.send(GuiCommand::RescanAircrafts { custom_prefix: pfx });
                    }
                    ui.label(
                        RichText::new(format!("{} MSFS Aircraft Indexed", aircrafts.len()))
                            .color(Color32::from_rgb(160, 200, 255)),
                    );
                });
            });

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Active MSFS Aircraft:").color(Color32::GRAY));
                ui.label(
                    RichText::new(active_aircraft)
                        .color(Color32::from_rgb(0, 230, 255))
                        .strong(),
                );
                ui.separator();
                ui.label(RichText::new("Active Profile:").color(Color32::GRAY));
                ui.label(
                    RichText::new(format!("{} ({})", active_profile_name, active_profile_path))
                        .color(Color32::from_rgb(0, 255, 140))
                        .strong(),
                );

                if let Some((ref msg, is_err)) = state.status_banner {
                    ui.separator();
                    let col = if is_err {
                        Color32::LIGHT_RED
                    } else {
                        Color32::from_rgb(100, 255, 150)
                    };
                    ui.label(RichText::new(msg).color(col));
                }
            });
        });

    ui.add_space(8.0);

    // Two-column split: Left = Installed MSFS Aircraft & Binder, Right = Categorized Profile Repository
    ui.columns(2, |columns| {
        render_left_aircraft_binder(
            &mut columns[0],
            state,
            aircrafts,
            assignments,
            &available_profiles,
            active_aircraft,
            active_profile_path,
            cmd_tx,
        );

        render_right_profile_repository(
            &mut columns[1],
            state,
            &repo_catalog,
            devices,
            active_profile_path,
            cmd_tx,
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn render_left_aircraft_binder(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    aircrafts: &[DiscoveredAircraft],
    assignments: &AircraftAssignmentManager,
    available_profiles: &[String],
    active_aircraft: &str,
    active_profile_path: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
) {
    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.heading(
                RichText::new("1. Installed MSFS Aircraft & Profile Bindings")
                    .color(Color32::from_rgb(130, 210, 255))
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "Select any installed MSFS aircraft below to bind it to a profile. When MSFS switches to that aircraft, AeroDeck automatically loads its bound profile.",
                )
                .small()
                .color(Color32::GRAY),
            );
            ui.add_space(6.0);

            // Search and Filter Controls
            ui.horizontal(|ui| {
                ui.label("🔍 Search:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.aircraft_search)
                        .hint_text("Filter by name, ICAO (A20N, B748, C172), manufacturer...")
                        .desired_width(220.0),
                );
                if !state.aircraft_search.is_empty() && ui.small_button("✖").clicked() {
                    state.aircraft_search.clear();
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut state.aircraft_filter_mode,
                    AircraftFilterMode::All,
                    format!("All ({})", aircrafts.len()),
                );
                let bound_count = aircrafts
                    .iter()
                    .filter(|a| assignments.assignments.contains_key(&a.title))
                    .count();
                ui.selectable_value(
                    &mut state.aircraft_filter_mode,
                    AircraftFilterMode::BoundOnly,
                    format!("Explicitly Bound ({})", bound_count),
                );
                ui.selectable_value(
                    &mut state.aircraft_filter_mode,
                    AircraftFilterMode::UnboundOnly,
                    "Unbound",
                );
                let comm_count = aircrafts
                    .iter()
                    .filter(|a| a.package_source.eq_ignore_ascii_case("Community"))
                    .count();
                ui.selectable_value(
                    &mut state.aircraft_filter_mode,
                    AircraftFilterMode::CommunityOnly,
                    format!("Community ({})", comm_count),
                );
            });

            ui.separator();

            // Quick Bind Bar for Selected Aircraft
            if let Some(ref sel_title) = state.selected_aircraft_title.clone() {
                egui::Frame::default()
                    .fill(Color32::from_rgb(30, 42, 58))
                    .inner_margin(8.0)
                    .rounding(4.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Selected Aircraft:").color(Color32::LIGHT_GRAY));
                            ui.label(
                                RichText::new(sel_title)
                                    .color(Color32::from_rgb(0, 230, 255))
                                    .strong(),
                            );
                        });

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            egui::ComboBox::from_id_salt("bind_target_profile_combo")
                                .selected_text(&state.selected_profile_to_bind)
                                .width(230.0)
                                .show_ui(ui, |ui| {
                                    for p in available_profiles {
                                        ui.selectable_value(
                                            &mut state.selected_profile_to_bind,
                                            p.clone(),
                                            p,
                                        );
                                    }
                                });

                            if ui
                                .button(
                                    RichText::new("🔗 Bind to Selected Profile")
                                        .color(Color32::from_rgb(0, 255, 140))
                                        .strong(),
                                )
                                .clicked()
                            {
                                let _ = cmd_tx.send(GuiCommand::BindAircraftToProfile {
                                    aircraft_title: sel_title.clone(),
                                    profile_path: state.selected_profile_to_bind.clone(),
                                });
                                state.status_banner = Some((
                                    format!(
                                        "Bound '{}' -> '{}'",
                                        sel_title, state.selected_profile_to_bind
                                    ),
                                    false,
                                ));
                            }

                            if !active_profile_path.is_empty()
                                && ui.button("📌 Bind to Active Profile").clicked()
                            {
                                let _ = cmd_tx.send(GuiCommand::BindAircraftToProfile {
                                    aircraft_title: sel_title.clone(),
                                    profile_path: active_profile_path.to_string(),
                                });
                                state.status_banner = Some((
                                    format!("Bound '{}' -> '{}'", sel_title, active_profile_path),
                                    false,
                                ));
                            }

                            if ui
                                .button(
                                    RichText::new("⚡ Activate Aircraft Now")
                                        .color(Color32::from_rgb(100, 220, 255)),
                                )
                                .on_hover_text(
                                    "Simulates switching to this aircraft in MSFS and triggers automatic profile loading",
                                )
                                .clicked()
                            {
                                let _ = cmd_tx.send(GuiCommand::SimulateAircraftSwitch(
                                    sel_title.clone(),
                                ));
                            }

                            if assignments.assignments.contains_key(sel_title)
                                && ui
                                    .button(RichText::new("✖ Unbind").color(Color32::LIGHT_RED))
                                    .clicked()
                            {
                                let _ = cmd_tx.send(GuiCommand::UnbindAircraft {
                                    aircraft_title: sel_title.clone(),
                                });
                            }
                        });
                    });
                ui.add_space(6.0);
            }

            // Aircraft Scroll List
            let query = state.aircraft_search.trim().to_lowercase();
            let filtered: Vec<&DiscoveredAircraft> = aircrafts
                .iter()
                .filter(|ac| {
                    match state.aircraft_filter_mode {
                        AircraftFilterMode::All => {}
                        AircraftFilterMode::BoundOnly => {
                            if !assignments.assignments.contains_key(&ac.title) {
                                return false;
                            }
                        }
                        AircraftFilterMode::UnboundOnly => {
                            if assignments.assignments.contains_key(&ac.title) {
                                return false;
                            }
                        }
                        AircraftFilterMode::CommunityOnly => {
                            if !ac.package_source.eq_ignore_ascii_case("Community") {
                                return false;
                            }
                        }
                    }
                    if query.is_empty() {
                        return true;
                    }
                    ac.title.to_lowercase().contains(&query)
                        || ac.icao.to_lowercase().contains(&query)
                        || ac.manufacturer.to_lowercase().contains(&query)
                        || ac.model.to_lowercase().contains(&query)
                        || ac.category.to_lowercase().contains(&query)
                        || ac.simobject_folder.to_lowercase().contains(&query)
                })
                .collect();

            ScrollArea::vertical()
                .id_salt("installed_aircraft_scroll")
                .max_height(ui.available_height() - 10.0)
                .show(ui, |ui| {
                    if filtered.is_empty() {
                        ui.label(
                            RichText::new("No matching aircraft found.")
                                .italics()
                                .color(Color32::GRAY),
                        );
                        return;
                    }

                    for ac in filtered {
                        let is_selected =
                            state.selected_aircraft_title.as_deref() == Some(ac.title.as_str());
                        let is_active_in_sim = active_aircraft.eq_ignore_ascii_case(&ac.title);
                        let explicit_binding = assignments.assignments.get(&ac.title);
                        let resolved_binding = assignments
                            .resolve_profile_for_aircraft(&ac.title, aircrafts)
                            .map(|p| p.to_string_lossy().to_string());

                        let bg_color = if is_active_in_sim {
                            Color32::from_rgb(20, 52, 42)
                        } else if is_selected {
                            Color32::from_rgb(36, 48, 68)
                        } else {
                            Color32::from_rgb(24, 27, 34)
                        };

                        egui::Frame::default()
                            .fill(bg_color)
                            .inner_margin(6.0)
                            .rounding(4.0)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    let title_color = if is_active_in_sim {
                                        Color32::from_rgb(0, 255, 140)
                                    } else if is_selected {
                                        Color32::from_rgb(0, 225, 255)
                                    } else {
                                        Color32::WHITE
                                    };

                                    if ui
                                        .selectable_label(
                                            is_selected,
                                            RichText::new(&ac.title).color(title_color).strong(),
                                        )
                                        .clicked()
                                    {
                                        state.selected_aircraft_title = Some(ac.title.clone());
                                        if let Some(bound) =
                                            explicit_binding.or(resolved_binding.as_ref())
                                        {
                                            state.selected_profile_to_bind = bound.clone();
                                        }
                                    }

                                    if !ac.icao.is_empty() {
                                        ui.label(
                                            RichText::new(format!("[{}]", ac.icao))
                                                .small()
                                                .color(Color32::from_rgb(130, 200, 255)),
                                        );
                                    }
                                    ui.label(
                                        RichText::new(format!(
                                            "{} • {}",
                                            ac.manufacturer, ac.category
                                        ))
                                        .small()
                                        .color(Color32::GRAY),
                                    );
                                    if ac.package_source.eq_ignore_ascii_case("Community") {
                                        ui.label(
                                            RichText::new("[Community]")
                                                .small()
                                                .color(Color32::from_rgb(255, 190, 80)),
                                        );
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button("⚡ Load")
                                                .on_hover_text(
                                                    "Switch active aircraft & auto-load bound profile",
                                                )
                                                .clicked()
                                            {
                                                state.selected_aircraft_title =
                                                    Some(ac.title.clone());
                                                let _ = cmd_tx.send(
                                                    GuiCommand::SimulateAircraftSwitch(
                                                        ac.title.clone(),
                                                    ),
                                                );
                                            }

                                            if let Some(bound_path) = explicit_binding {
                                                let short_name = bound_path
                                                    .rsplit('/')
                                                    .next()
                                                    .unwrap_or(bound_path.as_str());
                                                ui.label(
                                                    RichText::new(format!("🔗 {}", short_name))
                                                        .small()
                                                        .color(Color32::from_rgb(0, 255, 130))
                                                        .strong(),
                                                );
                                            } else if let Some(ref fallback_path) = resolved_binding
                                            {
                                                let short_name = fallback_path
                                                    .rsplit('/')
                                                    .next()
                                                    .unwrap_or(fallback_path.as_str());
                                                ui.label(
                                                    RichText::new(format!("↪ {}", short_name))
                                                        .small()
                                                        .color(Color32::from_rgb(150, 190, 230)),
                                                )
                                                .on_hover_text(
                                                    "Matched via aircraft family or default fallback profile",
                                                );
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(3.0);
                    }
                });
        });
}

fn render_right_profile_repository(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    repo_catalog: &[RepositoryProfileEntry],
    devices: &[ConnectedDevice],
    active_profile_path: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
) {
    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.heading(
                RichText::new("2. SPAD.neXt Online Repository & Profile Importer")
                    .color(Color32::from_rgb(0, 240, 170))
                    .strong(),
            );

            // Top SPAD.neXt Account & Automatic License Token Bar
            egui::Frame::default()
                .fill(Color32::from_rgb(20, 38, 34))
                .inner_margin(8.0)
                .rounding(4.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("🔑 SPAD.neXt License:")
                                .color(Color32::from_rgb(0, 255, 140))
                                .strong(),
                        );
                        let key_preview = if state.spad_account.license_key.len() > 14 {
                            format!("{}...", &state.spad_account.license_key[..14])
                        } else {
                            state.spad_account.license_key.clone()
                        };
                        ui.label(
                            RichText::new(format!(
                                "{} (Order #{} • Key {} • HWID {})",
                                state.spad_account.email,
                                state.spad_account.purchase_id,
                                key_preview,
                                state.spad_account.hardware_id
                            ))
                            .color(Color32::WHITE)
                            .strong(),
                        );
                        ui.label(
                            RichText::new("✔ Auto X-Token Active")
                                .small()
                                .color(Color32::from_rgb(100, 255, 160)),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let btn_label = if state.show_account_editor {
                                "▲ Hide License Settings"
                            } else {
                                "⚙ License / Account Settings"
                            };
                            if ui.small_button(btn_label).clicked() {
                                state.show_account_editor = !state.show_account_editor;
                            }
                        });
                    });

                    if state.show_account_editor {
                        ui.separator();
                        ui.label(
                            RichText::new(
                                "AeroDeck automatically generates the fresh SPAD.neXt X-Token header on every request using your Order/Purchase ID & Hardware ID — you never need to enter a token manually.",
                            )
                            .small()
                            .color(Color32::LIGHT_GRAY),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Email:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.spad_account.email)
                                    .desired_width(155.0),
                            );
                            ui.label("Order / Purchase ID:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.spad_account.purchase_id)
                                    .desired_width(85.0),
                            );
                            ui.label("Hardware ID:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.spad_account.hardware_id)
                                    .desired_width(175.0),
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("License Key:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.spad_account.license_key)
                                    .desired_width(310.0),
                            );
                            if ui
                                .button(
                                    RichText::new("💾 Save & Reconnect SPAD.neXt")
                                        .color(Color32::from_rgb(0, 255, 140)),
                                )
                                .clicked()
                            {
                                let _ = state.spad_account.save();
                                state.is_fetching_online = true;
                                let _ = cmd_tx.send(GuiCommand::FetchOnlineSpadCatalog {
                                    account: state.spad_account.clone(),
                                });
                                state.status_banner = Some((
                                    "Saved SPAD.neXt license & refreshing online profile catalog..."
                                        .to_string(),
                                    false,
                                ));
                            }
                            if ui.button("🔍 Auto-Detect My License").clicked() {
                                state.spad_account = SpadAccountConfig::load_or_autodiscover();
                                let _ = state.spad_account.save();
                                state.status_banner = Some((
                                    format!(
                                        "Auto-detected SPAD.neXt license for {} (Order #{})",
                                        state.spad_account.email, state.spad_account.purchase_id
                                    ),
                                    false,
                                ));
                            }
                        });
                    }
                });

            ui.add_space(6.0);

            // Three Prominent Tabs: Online SPAD.neXt Repository | My Windows SPAD Profiles | Built-in Verified
            ui.horizontal_wrapped(|ui| {
                let online_label = if state.online_catalog.is_empty() && state.is_fetching_online {
                    "🌐 SPAD.neXt Online Repository (Loading...)".to_string()
                } else {
                    format!(
                        "🌐 SPAD.neXt Online Repository ({} Profiles)",
                        state.online_catalog.len()
                    )
                };
                ui.selectable_value(
                    &mut state.active_repo_tab,
                    RepositorySourceTab::OnlineSpadRepository,
                    RichText::new(online_label).strong(),
                );

                ui.selectable_value(
                    &mut state.active_repo_tab,
                    RepositorySourceTab::WindowsSpadBackup,
                    RichText::new(format!(
                        "💻 My Windows SPAD Profiles ({})",
                        state.windows_profiles.len()
                    ))
                    .strong(),
                );

                ui.selectable_value(
                    &mut state.active_repo_tab,
                    RepositorySourceTab::BuiltinVerified,
                    RichText::new(format!("⭐ Built-in Verified ({})", repo_catalog.len()))
                        .strong(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("📂 Browse .xml / .spadprofile...").clicked() {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter("SPAD.neXt Profile", &["xml", "spadprofile"])
                            .pick_file()
                        {
                            let _ = cmd_tx.send(GuiCommand::ImportWindowsSpadProfile {
                                source_path: file,
                                activate_now: true,
                                bind_to_aircraft: state.selected_aircraft_title.clone(),
                            });
                        }
                    }
                });
            });

            ui.separator();

            match state.active_repo_tab {
                RepositorySourceTab::OnlineSpadRepository => {
                    render_online_spad_tab(ui, state, devices, active_profile_path, cmd_tx);
                }
                RepositorySourceTab::WindowsSpadBackup => {
                    render_windows_spad_tab(ui, state, devices, active_profile_path, cmd_tx);
                }
                RepositorySourceTab::BuiltinVerified => {
                    render_builtin_verified_tab(
                        ui,
                        state,
                        repo_catalog,
                        devices,
                        active_profile_path,
                        cmd_tx,
                    );
                }
            }
        });
}

fn render_online_spad_tab(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    devices: &[ConnectedDevice],
    active_profile_path: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
) {
    // Search & Device Filter Controls
    ui.horizontal_wrapped(|ui| {
        ui.label("🔍 Search Online:");
        ui.add(
            egui::TextEdit::singleline(&mut state.online_search)
                .hint_text("Search Fenix, A320, FBW, PMDG 737, 777, B787, Duke, C172, author...")
                .desired_width(240.0),
        );
        if !state.online_search.is_empty() && ui.small_button("✖").clicked() {
            state.online_search.clear();
        }

        ui.checkbox(
            &mut state.online_match_my_devices,
            RichText::new("★ Only My Connected Hardware")
                .color(Color32::from_rgb(255, 215, 70))
                .strong(),
        );

        if ui.button("🔄 Refresh Online Catalog").clicked() {
            state.is_fetching_online = true;
            let _ = cmd_tx.send(GuiCommand::FetchOnlineSpadCatalog {
                account: state.spad_account.clone(),
            });
        }
    });

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Quick Filter:").small().color(Color32::GRAY));
        for preset in [
            ("All", ""),
            ("Fenix A320", "Fenix"),
            ("FBW A32NX", "A32"),
            ("PMDG 737", "737"),
            ("PMDG 777", "777"),
            ("B787", "787"),
            ("C172 / GA", "172"),
            ("Winwing", "4098:"),
            ("Thrustmaster TCA", "044F:0407"),
        ] {
            let active = state.online_search.eq_ignore_ascii_case(preset.1);
            if ui.selectable_label(active, preset.0).clicked() {
                state.online_search = preset.1.to_string();
            }
        }
    });

    ui.separator();

    if state.online_catalog.is_empty() {
        if state.is_fetching_online {
            ui.label(
                RichText::new(
                    "⏳ Fetching 2,250+ profiles from update.spadnext.net using your SPAD.neXt license...",
                )
                .color(Color32::from_rgb(0, 225, 255)),
            );
        } else {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Online catalog not loaded yet.")
                        .italics()
                        .color(Color32::GRAY),
                );
                if ui.button("🌐 Load SPAD.neXt Online Profiles Now").clicked() {
                    state.is_fetching_online = true;
                    let _ = cmd_tx.send(GuiCommand::FetchOnlineSpadCatalog {
                        account: state.spad_account.clone(),
                    });
                }
            });
        }
        return;
    }

    let q = state.online_search.trim().to_lowercase();
    let filtered: Vec<&OnlineSpadProfileEntry> = state
        .online_catalog
        .iter()
        .filter(|entry| {
            if state.online_match_my_devices && !devices.is_empty() {
                let matches_conn = entry.devices.iter().any(|vp| {
                    devices.iter().any(|c| {
                        format!("{:04X}:{:04X}", c.vendor_id, c.product_id)
                            .eq_ignore_ascii_case(vp)
                            || (c.vendor_id == 0x4098 && vp.starts_with("4098:"))
                    })
                });
                if !matches_conn {
                    return false;
                }
            }
            if q.is_empty() {
                return true;
            }
            entry.name.to_lowercase().contains(&q)
                || entry.author.to_lowercase().contains(&q)
                || entry.comment.to_lowercase().contains(&q)
                || entry.aircrafts.iter().any(|a| a.to_lowercase().contains(&q))
                || entry.devices.iter().any(|d| d.to_lowercase().contains(&q))
        })
        .collect();

    ui.label(
        RichText::new(format!(
            "Showing {} of {} matching online profiles from update.spadnext.net (sorted by popularity):",
            filtered.len().min(150),
            filtered.len()
        ))
        .small()
        .color(Color32::GRAY),
    );
    ui.add_space(4.0);

    ScrollArea::vertical()
        .id_salt("online_spad_catalog_scroll")
        .max_height(ui.available_height() - 10.0)
        .show(ui, |ui| {
            for entry in filtered.into_iter().take(150) {
                egui::Frame::default()
                    .fill(Color32::from_rgb(28, 33, 44))
                    .inner_margin(8.0)
                    .rounding(5.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(&entry.name)
                                    .color(Color32::from_rgb(0, 230, 255))
                                    .strong()
                                    .size(15.0),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "by {} • v{} • {}",
                                    entry.author, entry.version, entry.published
                                ))
                                .small()
                                .color(Color32::LIGHT_GRAY),
                            );
                            if !entry.features.is_empty() {
                                ui.label(
                                    RichText::new(format!("[{}]", entry.features))
                                        .small()
                                        .color(Color32::from_rgb(255, 200, 90)),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "⬇ {} dl  •  ★ {}",
                                            entry.dlcount, entry.rating
                                        ))
                                        .small()
                                        .color(Color32::from_rgb(255, 215, 0)),
                                    );
                                },
                            );
                        });

                        if !entry.devices.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("Devices:").small().color(Color32::GRAY));
                                for vp in entry.devices.iter().take(8) {
                                    let is_plugged_in = devices.iter().any(|c| {
                                        format!("{:04X}:{:04X}", c.vendor_id, c.product_id)
                                            .eq_ignore_ascii_case(vp)
                                    });
                                    let dev_label = EXTRACTED_USB_DEVICES
                                        .iter()
                                        .find(|d| d.vid_pid.eq_ignore_ascii_case(vp))
                                        .map(|d| format!("{} {}", d.vendor_name, d.product_name))
                                        .unwrap_or_else(|| vp.clone());
                                    let (col, pfx) = if is_plugged_in {
                                        (Color32::from_rgb(0, 255, 130), "✔ ")
                                    } else {
                                        (Color32::LIGHT_GRAY, "• ")
                                    };
                                    ui.label(
                                        RichText::new(format!("{}{} [{}]", pfx, dev_label, vp))
                                            .small()
                                            .color(col),
                                    );
                                }
                            });
                        }

                        if !entry.aircrafts.is_empty() {
                            let ac_preview = entry
                                .aircrafts
                                .iter()
                                .take(4)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(
                                RichText::new(format!("Aircraft: {}", ac_preview))
                                    .small()
                                    .color(Color32::from_rgb(130, 195, 250)),
                            );
                        }

                        if !entry.comment.trim().is_empty() {
                            let short_comment = if entry.comment.len() > 220 {
                                format!("{}...", &entry.comment[..220])
                            } else {
                                entry.comment.clone()
                            };
                            ui.label(
                                RichText::new(short_comment)
                                    .small()
                                    .color(Color32::from_rgb(195, 200, 210)),
                            );
                        }

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(
                                    RichText::new("📥 Download & Activate Now")
                                        .color(Color32::from_rgb(0, 255, 140))
                                        .strong(),
                                )
                                .clicked()
                            {
                                let _ = cmd_tx.send(GuiCommand::DownloadOnlineSpadSnippet {
                                    snippet_id: entry.snippet_id,
                                    profile_name: entry.name.clone(),
                                    account: state.spad_account.clone(),
                                    activate_now: true,
                                    bind_to_aircraft: None,
                                });
                                state.status_banner = Some((
                                    format!(
                                        "Downloading SPAD.neXt Profile #{} '{}'...",
                                        entry.snippet_id, entry.name
                                    ),
                                    false,
                                ));
                            }

                            if let Some(ref sel_ac) = state.selected_aircraft_title.clone() {
                                if ui
                                    .button(
                                        RichText::new(format!(
                                            "🔗 Download & Bind to '{}'",
                                            sel_ac
                                        ))
                                        .color(Color32::from_rgb(100, 220, 255)),
                                    )
                                    .clicked()
                                {
                                    let _ = cmd_tx.send(GuiCommand::DownloadOnlineSpadSnippet {
                                        snippet_id: entry.snippet_id,
                                        profile_name: entry.name.clone(),
                                        account: state.spad_account.clone(),
                                        activate_now: true,
                                        bind_to_aircraft: Some(sel_ac.clone()),
                                    });
                                    state.status_banner = Some((
                                        format!(
                                            "Downloading '{}' and binding to '{}'...",
                                            entry.name, sel_ac
                                        ),
                                        false,
                                    ));
                                }
                            }

                            let _ = active_profile_path;
                        });
                    });
                ui.add_space(6.0);
            }
        });
}

fn render_windows_spad_tab(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    devices: &[ConnectedDevice],
    active_profile_path: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Windows SPAD.neXt Folder:").color(Color32::LIGHT_GRAY));
        ui.add(
            egui::TextEdit::singleline(&mut state.spad_account.windows_spad_dir)
                .desired_width(310.0),
        );
        if ui.button("📂 Pick Folder...").clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                state.spad_account.windows_spad_dir = dir.to_string_lossy().to_string();
                let _ = state.spad_account.save();
                let _ = cmd_tx.send(GuiCommand::ImportAllWindowsSpadProfiles {
                    custom_dir: Some(state.spad_account.windows_spad_dir.clone()),
                });
            }
        }
        if ui
            .button(
                RichText::new("📥 Import All to AeroDeck")
                    .color(Color32::from_rgb(0, 255, 140))
                    .strong(),
            )
            .clicked()
        {
            let _ = cmd_tx.send(GuiCommand::ImportAllWindowsSpadProfiles {
                custom_dir: Some(state.spad_account.windows_spad_dir.clone()),
            });
            state.status_banner = Some((
                "Imported all Windows SPAD.neXt profiles into ./profiles/".to_string(),
                false,
            ));
        }
    });

    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "These are your personal SPAD.neXt profiles discovered from your Windows backup/installation. They are automatically imported into ./profiles/ so you can activate or bind them to any aircraft in 1 click!",
        )
        .small()
        .color(Color32::GRAY),
    );
    ui.separator();

    let win_list = state.windows_profiles.clone();
    ScrollArea::vertical()
        .id_salt("windows_spad_profiles_scroll")
        .max_height(ui.available_height() - 10.0)
        .show(ui, |ui| {
            if win_list.is_empty() {
                ui.label(
                    RichText::new(
                        "No Windows SPAD.neXt profiles found in the specified folder. Click 'Pick Folder...' to select your Windows Documents/SPAD.neXt/profiles directory.",
                    )
                    .italics()
                    .color(Color32::GRAY),
                );
                return;
            }

            for wp in &win_list {
                let rel_file = format!("profiles/{}", wp.filename);
                let is_active = active_profile_path == rel_file
                    || active_profile_path.ends_with(&wp.filename);
                let bg = if is_active {
                    Color32::from_rgb(20, 48, 40)
                } else {
                    Color32::from_rgb(28, 32, 42)
                };

                egui::Frame::default()
                    .fill(bg)
                    .inner_margin(8.0)
                    .rounding(5.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(&wp.name)
                                    .color(Color32::from_rgb(0, 230, 255))
                                    .strong()
                                    .size(15.0),
                            );
                            ui.label(
                                RichText::new(format!("({})", wp.filename))
                                    .small()
                                    .color(Color32::GRAY),
                            );
                            if is_active {
                                ui.label(
                                    RichText::new("● ACTIVE PROFILE")
                                        .small()
                                        .color(Color32::from_rgb(0, 255, 130))
                                        .strong(),
                                );
                            } else if wp.already_imported {
                                ui.label(
                                    RichText::new("✔ Imported in ./profiles/")
                                        .small()
                                        .color(Color32::from_rgb(120, 220, 160)),
                                );
                            }
                            if !wp.last_changed.is_empty() {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format!("Modified: {}", wp.last_changed))
                                                .small()
                                                .color(Color32::GRAY),
                                        );
                                    },
                                );
                            }
                        });

                        if !wp.devices.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("Devices:").small().color(Color32::GRAY));
                                for vp in &wp.devices {
                                    let is_plugged_in = devices.iter().any(|c| {
                                        format!("{:04X}:{:04X}", c.vendor_id, c.product_id)
                                            .eq_ignore_ascii_case(vp)
                                    });
                                    let dev_label = EXTRACTED_USB_DEVICES
                                        .iter()
                                        .find(|d| d.vid_pid.eq_ignore_ascii_case(vp))
                                        .map(|d| format!("{} {}", d.vendor_name, d.product_name))
                                        .unwrap_or_else(|| vp.clone());
                                    let (col, pfx) = if is_plugged_in {
                                        (Color32::from_rgb(0, 255, 130), "✔ ")
                                    } else {
                                        (Color32::LIGHT_GRAY, "• ")
                                    };
                                    ui.label(
                                        RichText::new(format!("{}{} [{}]", pfx, dev_label, vp))
                                            .small()
                                            .color(col),
                                    );
                                }
                            });
                        }

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(
                                    RichText::new("⚡ Import & Activate Now")
                                        .color(Color32::from_rgb(0, 255, 140))
                                        .strong(),
                                )
                                .clicked()
                            {
                                let _ = cmd_tx.send(GuiCommand::ImportWindowsSpadProfile {
                                    source_path: wp.source_path.clone(),
                                    activate_now: true,
                                    bind_to_aircraft: None,
                                });
                                state.status_banner = Some((
                                    format!("Activated Windows SPAD profile '{}'", wp.name),
                                    false,
                                ));
                            }

                            if let Some(ref sel_ac) = state.selected_aircraft_title.clone() {
                                if ui
                                    .button(
                                        RichText::new(format!(
                                            "🔗 Import & Bind to '{}'",
                                            sel_ac
                                        ))
                                        .color(Color32::from_rgb(100, 220, 255)),
                                    )
                                    .clicked()
                                {
                                    let _ = cmd_tx.send(GuiCommand::ImportWindowsSpadProfile {
                                        source_path: wp.source_path.clone(),
                                        activate_now: true,
                                        bind_to_aircraft: Some(sel_ac.clone()),
                                    });
                                    state.status_banner = Some((
                                        format!("Bound '{}' -> '{}'", sel_ac, rel_file),
                                        false,
                                    ));
                                }
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        });
}

fn render_builtin_verified_tab(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    repo_catalog: &[RepositoryProfileEntry],
    devices: &[ConnectedDevice],
    active_profile_path: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(
            &mut state.match_connected_hardware_only,
            RichText::new("★ Match My Connected Hardware")
                .color(Color32::from_rgb(255, 215, 70))
                .strong(),
        );

        ui.separator();
        ui.label("Aircraft Class:");
        for cat in [
            "All",
            "Airliner",
            "General Aviation",
            "Turboprop",
            "Military",
            "Rotorcraft",
            "Saitek",
        ] {
            ui.selectable_value(&mut state.repo_category_filter, cat.to_string(), cat);
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("🔍 Filter Profiles:");
        ui.add(
            egui::TextEdit::singleline(&mut state.repo_search)
                .hint_text("Search A320, Boeing, Cessna, Winwing, TCA, Saitek...")
                .desired_width(240.0),
        );
        if !state.repo_search.is_empty() && ui.small_button("✖").clicked() {
            state.repo_search.clear();
        }
    });

    ui.separator();

    ScrollArea::vertical()
        .id_salt("profile_repository_scroll")
        .max_height(ui.available_height() - 10.0)
        .show(ui, |ui| {
            let q = state.repo_search.trim().to_lowercase();
            for entry in repo_catalog {
                let hw_matches = entry.vendor_product_ids.iter().any(|vid_pid| {
                    devices.iter().any(|conn| {
                        let conn_vp =
                            format!("{:04X}:{:04X}", conn.vendor_id, conn.product_id);
                        conn_vp.eq_ignore_ascii_case(vid_pid)
                    })
                });

                if state.match_connected_hardware_only && !hw_matches && !devices.is_empty() {
                    continue;
                }

                if state.repo_category_filter != "All"
                    && !entry
                        .aircraft_category
                        .to_lowercase()
                        .contains(&state.repo_category_filter.to_lowercase())
                    && !entry
                        .device_category
                        .to_lowercase()
                        .contains(&state.repo_category_filter.to_lowercase())
                {
                    continue;
                }

                if !q.is_empty()
                    && !entry.name.to_lowercase().contains(&q)
                    && !entry.description.to_lowercase().contains(&q)
                    && !entry.aircraft_category.to_lowercase().contains(&q)
                {
                    continue;
                }

                let rel_file = format!("profiles/{}", entry.filename);
                let is_currently_active = active_profile_path == rel_file
                    || active_profile_path.ends_with(&entry.filename);

                let card_bg = if is_currently_active {
                    Color32::from_rgb(20, 48, 40)
                } else {
                    Color32::from_rgb(28, 32, 42)
                };

                egui::Frame::default()
                    .fill(card_bg)
                    .inner_margin(8.0)
                    .rounding(5.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(&entry.name)
                                    .color(Color32::from_rgb(0, 230, 255))
                                    .strong()
                                    .size(15.0),
                            );
                            ui.label(
                                RichText::new(format!("[{}]", entry.aircraft_category))
                                    .small()
                                    .color(Color32::from_rgb(255, 200, 90)),
                            );
                            if is_currently_active {
                                ui.label(
                                    RichText::new("● ACTIVE PROFILE")
                                        .small()
                                        .color(Color32::from_rgb(0, 255, 130))
                                        .strong(),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "★ {:.1} ({} dl)",
                                            entry.rating, entry.downloads
                                        ))
                                        .small()
                                        .color(Color32::from_rgb(255, 215, 0)),
                                    );
                                },
                            );
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Devices:").small().color(Color32::GRAY));
                            for vp in &entry.vendor_product_ids {
                                let is_plugged_in = devices.iter().any(|c| {
                                    format!("{:04X}:{:04X}", c.vendor_id, c.product_id)
                                        .eq_ignore_ascii_case(vp)
                                });
                                let dev_label = EXTRACTED_USB_DEVICES
                                    .iter()
                                    .find(|d| d.vid_pid.eq_ignore_ascii_case(vp))
                                    .map(|d| format!("{} {}", d.vendor_name, d.product_name))
                                    .unwrap_or_else(|| vp.clone());

                                let (badge_col, prefix) = if is_plugged_in {
                                    (Color32::from_rgb(0, 255, 130), "✔ ")
                                } else {
                                    (Color32::LIGHT_GRAY, "• ")
                                };
                                ui.label(
                                    RichText::new(format!("{}{} [{}]", prefix, dev_label, vp))
                                        .small()
                                        .color(badge_col),
                                );
                            }
                        });

                        ui.label(
                            RichText::new(&entry.description)
                                .small()
                                .color(Color32::from_rgb(200, 205, 215)),
                        );

                        if !entry.recommended_aircraft_titles.is_empty() {
                            ui.label(
                                RichText::new(format!(
                                    "Auto-binds to: {}",
                                    entry.recommended_aircraft_titles.join(", ")
                                ))
                                .small()
                                .color(Color32::from_rgb(130, 185, 240)),
                            );
                        }

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button(
                                    RichText::new("📥 Download & Activate Now")
                                        .color(Color32::from_rgb(0, 255, 140))
                                        .strong(),
                                )
                                .clicked()
                            {
                                let _ = cmd_tx.send(GuiCommand::InstallRepositoryProfile {
                                    repo_id: entry.id.clone(),
                                    activate_now: true,
                                    bind_to_aircraft: None,
                                });
                                state.status_banner = Some((
                                    format!(
                                        "Downloaded & activated '{}' (profiles/{})",
                                        entry.name, entry.filename
                                    ),
                                    false,
                                ));
                            }

                            if let Some(ref sel_ac) = state.selected_aircraft_title.clone() {
                                if ui
                                    .button(
                                        RichText::new(format!(
                                            "🔗 Download & Bind to '{}'",
                                            sel_ac
                                        ))
                                        .color(Color32::from_rgb(100, 220, 255)),
                                    )
                                    .clicked()
                                {
                                    let _ = cmd_tx.send(GuiCommand::InstallRepositoryProfile {
                                        repo_id: entry.id.clone(),
                                        activate_now: true,
                                        bind_to_aircraft: Some(sel_ac.clone()),
                                    });
                                    state.status_banner = Some((
                                        format!(
                                            "Downloaded '{}' and bound to '{}'",
                                            entry.name, sel_ac
                                        ),
                                        false,
                                    ));
                                }
                            }
                        });
                    });
                ui.add_space(6.0);
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Direct XML URL:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.direct_url_input)
                        .hint_text("https://.../profile.xml")
                        .desired_width(210.0),
                );
                ui.label("Save as:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.direct_url_save_name)
                        .desired_width(130.0),
                );
                if ui.button("📥 Download URL").clicked()
                    && !state.direct_url_input.trim().is_empty()
                {
                    let _ = cmd_tx.send(GuiCommand::ImportProfileFromUrl {
                        url: state.direct_url_input.trim().to_string(),
                        save_name: state.direct_url_save_name.trim().to_string(),
                        activate_now: true,
                    });
                }
            });
        });
}
