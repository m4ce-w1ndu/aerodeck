use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::gui::backend::GuiCommand;
use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::{
    AircraftAssignmentManager, DiscoveredAircraft, ProfileRepository, RepositoryProfileEntry,
    EXTRACTED_USB_DEVICES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AircraftFilterMode {
    All,
    BoundOnly,
    UnboundOnly,
    CommunityOnly,
}

pub struct AircraftProfilesViewState {
    pub aircraft_search: String,
    pub aircraft_filter_mode: AircraftFilterMode,
    pub selected_aircraft_title: Option<String>,
    pub selected_profile_to_bind: String,

    // Repository filter state
    pub match_connected_hardware_only: bool,
    pub repo_device_filter: String,
    pub repo_category_filter: String,
    pub repo_search: String,

    // Online SOAP & Direct URL state
    pub direct_url_input: String,
    pub direct_url_save_name: String,
    pub soap_search_query: String,
    pub soap_vid_input: String,
    pub soap_pid_input: String,
    pub soap_token_input: String,
    pub online_soap_response: String,
    pub status_banner: Option<(String, bool)>,
}

impl Default for AircraftProfilesViewState {
    fn default() -> Self {
        Self {
            aircraft_search: String::new(),
            aircraft_filter_mode: AircraftFilterMode::All,
            selected_aircraft_title: None,
            selected_profile_to_bind: "profiles/airbus_a320_winwing_tca.xml".to_string(),
            match_connected_hardware_only: true,
            repo_device_filter: "All".to_string(),
            repo_category_filter: "All".to_string(),
            repo_search: String::new(),
            direct_url_input: String::new(),
            direct_url_save_name: "downloaded_profile.xml".to_string(),
            soap_search_query: "A320".to_string(),
            soap_vid_input: "0x4098".to_string(),
            soap_pid_input: "0xBC27".to_string(),
            soap_token_input: String::new(),
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
                RichText::new("2. Categorized Profile Repository & Downloader")
                    .color(Color32::from_rgb(0, 240, 170))
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "Browse working SPAD.neXt-compatible profiles categorized by your connected flight controllers and aircraft class. Click 'Download & Activate' to install into ./profiles/ and load immediately.",
                )
                .small()
                .color(Color32::GRAY),
            );
            ui.add_space(6.0);

            // Connected hardware match filter
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

                if ui
                    .button("📂 Import Local .xml / .spadprofile...")
                    .clicked()
                {
                    if let Some(file) = rfd::FileDialog::new()
                        .add_filter("SPAD.neXt Profile", &["xml", "spadprofile"])
                        .pick_file()
                    {
                        let _ = cmd_tx.send(GuiCommand::LoadProfile(file));
                    }
                }
            });

            ui.separator();

            // Scrollable Repository Cards + Collapsible Online SOAP / URL section
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

                        if state.match_connected_hardware_only && !hw_matches && !devices.is_empty()
                        {
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

                                // Hardware Device Compatibility Badges
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new("Devices:").small().color(Color32::GRAY),
                                    );
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
                                            RichText::new(format!(
                                                "{}{} [{}]",
                                                prefix, dev_label, vp
                                            ))
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

                                    if let Some(ref sel_ac) = state.selected_aircraft_title.clone()
                                    {
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
                                            let _ =
                                                cmd_tx.send(GuiCommand::InstallRepositoryProfile {
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

                    // Collapsible section for Online SPAD.neXt SOAP Service & Direct URL Import
                    egui::CollapsingHeader::new(
                        RichText::new(
                            "🌐 Online SPAD.neXt SOAP Service (update.spadnext.net) & Direct URL Importer",
                        )
                        .color(Color32::from_rgb(160, 200, 255)),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "Import any .xml / .spadprofile directly from a web URL, or query the official SPAD.neXt SOAP 1.1 endpoint (https://update.spadnext.net/service/UpdateSvc2.asmx) with your SPAD.neXt account X-Token.",
                            )
                            .small()
                            .color(Color32::GRAY),
                        );
                        ui.add_space(4.0);

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

                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label("SOAP Search:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.soap_search_query)
                                    .desired_width(90.0),
                            );
                            ui.label("VID:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.soap_vid_input)
                                    .desired_width(60.0),
                            );
                            ui.label("PID:");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.soap_pid_input)
                                    .desired_width(60.0),
                            );
                            ui.label("X-Token (optional):");
                            ui.add(
                                egui::TextEdit::singleline(&mut state.soap_token_input)
                                    .password(true)
                                    .desired_width(100.0),
                            );
                            if ui.button("🔍 Query SOAP").clicked() {
                                let tok = if state.soap_token_input.trim().is_empty() {
                                    None
                                } else {
                                    Some(state.soap_token_input.trim().to_string())
                                };
                                let _ = cmd_tx.send(GuiCommand::QuerySpadOnlineSoap {
                                    search_query: state.soap_search_query.clone(),
                                    device_vid: state.soap_vid_input.clone(),
                                    device_pid: state.soap_pid_input.clone(),
                                    license_token: tok,
                                });
                            }
                        });
                    });
                });
        });
}
