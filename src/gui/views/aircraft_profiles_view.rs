use eframe::egui::{self, RichText, ScrollArea, Ui};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::gui::backend::GuiCommand;
use crate::gui::theme::AeroTheme;
use crate::gui::widgets::{
    ui_card, ui_clickable_card, ui_elevated_card, ui_status_badge, ui_tag_pill,
};
use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::schema::{DeviceProfile, SpadProfile};
use crate::profile::{
    AircraftAssignmentManager, DiscoveredAircraft, OnlineSpadProfileEntry, ProfileParser,
    ProfileRepository, SpadAccountConfig, WindowsSpadProfileEntry,
};

#[derive(Debug, Clone)]
pub struct SnippetOverwriteConfirm {
    pub device_name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bindings_count: usize,
    pub downloaded_layout: DeviceProfile,
    pub profile_path: String,
}

#[derive(Debug, Clone)]
pub struct SnippetUndoData {
    pub profile_path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub previous_layout: DeviceProfile,
}

pub struct AircraftProfilesViewState {
    pub selected_profile_path: Option<String>,
    pub profile_search: String,
    pub active_device_idx: usize,

    // Dialogs / Inline states
    pub is_creating_new: bool,
    pub new_profile_name: String,
    pub is_renaming: bool,
    pub rename_profile_input: String,
    pub new_aircraft_pattern: String,
    pub is_duplicating: bool,

    // Snippet import & confirmation
    pub snippet_browser_open: bool,
    pub snippet_search: String,
    pub confirm_overwrite: Option<SnippetOverwriteConfirm>,
    pub undo_last_import: Option<SnippetUndoData>,

    // SPAD.neXt Account & Automatic X-Token config
    pub spad_account: SpadAccountConfig,
    pub is_fetching_online: bool,
    pub online_catalog: Vec<OnlineSpadProfileEntry>,
    pub online_match_my_devices: bool,

    // Status notification / banner
    pub status_banner: Option<(String, bool)>,
    pub online_soap_response: String,
    pub windows_profiles: Vec<WindowsSpadProfileEntry>,
}

impl Default for AircraftProfilesViewState {
    fn default() -> Self {
        let spad_account = SpadAccountConfig::load_or_autodiscover();
        let windows_profiles = ProfileRepository::discover_windows_spad_profiles(Some(
            &spad_account.windows_spad_dir,
        ));
        Self {
            selected_profile_path: None,
            profile_search: String::new(),
            active_device_idx: 0,
            is_creating_new: false,
            new_profile_name: String::new(),
            is_renaming: false,
            rename_profile_input: String::new(),
            new_aircraft_pattern: String::new(),
            is_duplicating: false,
            snippet_browser_open: false,
            snippet_search: String::new(),
            confirm_overwrite: None,
            undo_last_import: None,
            spad_account,
            is_fetching_online: false,
            online_catalog: Vec::new(),
            online_match_my_devices: true,
            status_banner: None,
            online_soap_response: String::new(),
            windows_profiles,
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
    _custom_prefix: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    let available_profiles = ProfileRepository::list_local_profiles();

    // Default selection to active profile if none selected
    if state.selected_profile_path.is_none() && !available_profiles.is_empty() {
        if available_profiles.iter().any(|p| p == active_profile_path) {
            state.selected_profile_path = Some(active_profile_path.to_string());
        } else {
            state.selected_profile_path = Some(available_profiles[0].clone());
        }
    }

    // Top Header Card
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("✈ Aircraft Profiles")
                    .color(theme.accent)
                    .strong(),
            );
            ui.label(
                RichText::new("Manage your local aircraft profiles, auto-switching patterns, and controller layouts.")
                    .color(theme.text_secondary),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Auto-switch toggle
                let mut auto_switch = assignments.auto_switch_enabled;
                if ui
                    .checkbox(
                        &mut auto_switch,
                        RichText::new("⚡ Auto-Switch Profiles (MSFS)")
                            .color(theme.success)
                            .strong(),
                    )
                    .on_hover_text("Automatically loads the matching profile when you switch aircraft in the flight simulator")
                    .changed()
                {
                    let _ = cmd_tx.send(GuiCommand::SetAutoSwitchProfile(auto_switch));
                }

                if ui
                    .button(RichText::new("📂 Import Profile (.xml)...").color(theme.text_primary))
                    .on_hover_text("Import an existing SPAD.neXt or AeroDeck profile file into ./profiles/")
                    .clicked()
                {
                    if let Some(file) = rfd::FileDialog::new()
                        .add_filter("Profile XML", &["xml", "spadprofile"])
                        .pick_file()
                    {
                        if let Some(name) = file.file_name() {
                            let dest = Path::new("profiles").join(name);
                            let _ = fs::copy(&file, &dest);
                            let dest_str = dest.to_string_lossy().to_string();
                            state.selected_profile_path = Some(dest_str.clone());
                            let _ = cmd_tx.send(GuiCommand::LoadProfile(dest));
                            state.status_banner = Some((format!("Imported profile '{}'", name.to_string_lossy()), false));
                        }
                    }
                }

                if ui
                    .button(RichText::new("➕ New Profile").color(theme.accent).strong())
                    .on_hover_text("Create a new empty profile for an aircraft")
                    .clicked()
                {
                    state.is_creating_new = true;
                    state.new_profile_name = "New Aircraft Profile".to_string();
                }
            });
        });

        // Inline New Profile Creator
        if state.is_creating_new {
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Profile Name:").color(theme.text_primary).strong());
                ui.add(egui::TextEdit::singleline(&mut state.new_profile_name).desired_width(220.0));
                if ui.button(RichText::new("✔ Create Profile").color(theme.success).strong()).clicked() {
                    let clean = state.new_profile_name.trim().to_string();
                    if !clean.is_empty() {
                        let filename: String = clean
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
                            .collect();
                        let dest = PathBuf::from(format!("profiles/{}.xml", filename.trim()));
                        let mut new_prof = SpadProfile {
                            version: Some(2),
                            is_stock_profile: Some(false),
                            name: Some(clean.clone()),
                            variables: None,
                            options: None,
                            assigned_aircrafts: None,
                            devices: Some(crate::profile::schema::DeviceList { items: Vec::new() }),
                            profile_info: None,
                        };
                        // Add connected hardware devices as empty initial layouts
                        for d in devices {
                            new_prof.add_device_for_hardware(d.vendor_id, d.product_id, &d.name);
                        }
                        if ProfileParser::save_file(&dest, &new_prof).is_ok() {
                            let dest_str = dest.to_string_lossy().to_string();
                            state.selected_profile_path = Some(dest_str.clone());
                            let _ = cmd_tx.send(GuiCommand::LoadProfile(dest));
                            state.status_banner = Some((format!("Created new profile '{}'", clean), false));
                        }
                        state.is_creating_new = false;
                    }
                }
                if ui.button("Cancel").clicked() {
                    state.is_creating_new = false;
                }
            });
        }
    });

    // Optional Status Toast / Notification Banner
    let mut dismiss_status = false;
    if let Some((ref msg, is_err)) = state.status_banner {
        ui.add_space(4.0);
        let border = if is_err { theme.danger } else { theme.success };
        ui_elevated_card(ui, theme, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(if is_err { "⚠" } else { "✔" }).color(border).strong());
                ui.label(RichText::new(msg).color(theme.text_primary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        dismiss_status = true;
                    }
                });
            });
        });
    }
    if dismiss_status {
        state.status_banner = None;
    }

    ui.add_space(8.0);

    // Two-Column Main Workspace with full available height
    let avail_height = ui.available_height();
    ui.horizontal_top(|ui| {
        // LEFT COLUMN: Profiles List
        ui.allocate_ui_with_layout(
            egui::vec2(320.0, avail_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                render_profiles_list_panel(ui, state, &available_profiles, assignments, active_profile_path, cmd_tx, theme);
            },
        );

        ui.separator();

        // RIGHT COLUMN: Selected Profile Detail View
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), avail_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ScrollArea::vertical()
                    .id_salt("profile_details_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_profile_details_panel(
                            ui,
                            state,
                            aircrafts,
                            assignments,
                            devices,
                            active_aircraft,
                            active_profile_name,
                            active_profile_path,
                            cmd_tx,
                            theme,
                        );
                    });
            },
        );
    });
}

fn render_profiles_list_panel(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    available_profiles: &[String],
    assignments: &AircraftAssignmentManager,
    active_profile_path: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("PROFILES").color(theme.text_secondary).strong().small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("{} local", available_profiles.len())).color(theme.text_muted).small());
        });
    });
    ui.add_space(4.0);

    // Search bar
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.profile_search)
                .hint_text("🔍 Search profiles or aircraft...")
                .desired_width(ui.available_width() - 28.0),
        );
        if !state.profile_search.is_empty() && ui.small_button("✕").clicked() {
            state.profile_search.clear();
        }
    });

    ui.add_space(6.0);

    let search_lower = state.profile_search.trim().to_lowercase();
    let filtered_profiles: Vec<&String> = available_profiles
        .iter()
        .filter(|p| {
            if search_lower.is_empty() {
                return true;
            }
            if p.to_lowercase().contains(&search_lower) {
                return true;
            }
            // Also search aircraft patterns
            if let Ok(prof) = ProfileParser::parse_file(Path::new(p)) {
                if let Some(ref name) = prof.name {
                    if name.to_lowercase().contains(&search_lower) {
                        return true;
                    }
                }
                for pat in prof.aircraft_patterns() {
                    if pat.to_lowercase().contains(&search_lower) {
                        return true;
                    }
                }
            }
            false
        })
        .collect();

    ScrollArea::vertical()
        .id_salt("profiles_list_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if filtered_profiles.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No profiles found").color(theme.text_muted).italics());
                });
                return;
            }

            for p_path in filtered_profiles {
                let is_active = p_path == active_profile_path || active_profile_path.ends_with(p_path);
                let is_selected = state.selected_profile_path.as_deref() == Some(p_path.as_str());
                let is_fallback = assignments.default_profile.as_deref() == Some(p_path.as_str());

                // Read profile header metadata
                let prof_meta = ProfileParser::parse_file(Path::new(p_path)).ok();
                let prof_name = prof_meta
                    .as_ref()
                    .and_then(|p| p.name.clone())
                    .unwrap_or_else(|| {
                        Path::new(p_path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(p_path)
                            .replace('_', " ")
                    });

                let patterns = prof_meta
                    .as_ref()
                    .map(|p| p.aircraft_patterns())
                    .unwrap_or_default();

                let (device_count, mapped_count) = prof_meta
                    .as_ref()
                    .and_then(|p| p.devices.as_ref())
                    .map(|d| {
                        let dev_c = d.items.len();
                        let map_c: usize = d.items.iter().map(|dev| dev.mapped_input_count()).sum();
                        (dev_c, map_c)
                    })
                    .unwrap_or((0, 0));

                let card_builder = |ui: &mut Ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&prof_name)
                                .color(if is_selected { theme.accent } else { theme.text_primary })
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if is_active {
                                ui_tag_pill(ui, "ACTIVE", theme.success, theme);
                            }
                            if is_fallback {
                                ui_tag_pill(ui, "FALLBACK", theme.warning, theme);
                            }
                        });
                    });

                    ui.add_space(2.0);

                    // Aircraft pattern preview
                    let pattern_summary = if !patterns.is_empty() {
                        format!("✈ Matches: {}", patterns.join(", "))
                    } else if is_fallback {
                        "✈ Generic fallback for any aircraft".to_string()
                    } else {
                        "✈ No aircraft patterns configured".to_string()
                    };
                    ui.label(
                        RichText::new(pattern_summary)
                            .color(theme.text_secondary)
                            .small(),
                    );

                    ui.add_space(2.0);

                    // Device summary line + activate button
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("🎮 {} controllers • {} inputs", device_count, mapped_count))
                                .color(theme.text_muted)
                                .small(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !is_active && ui.small_button(RichText::new("Activate").color(theme.accent)).clicked() {
                                let _ = cmd_tx.send(GuiCommand::LoadProfile(PathBuf::from(p_path)));
                            }
                        });
                    });
                };

                let (_, resp) = ui_clickable_card(ui, theme, is_selected, card_builder);

                if resp.clicked() {
                    state.selected_profile_path = Some(p_path.clone());
                    state.active_device_idx = 0;
                    state.snippet_browser_open = false;
                    state.confirm_overwrite = None;
                }

                ui.add_space(4.0);
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn render_profile_details_panel(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    aircrafts: &[DiscoveredAircraft],
    assignments: &AircraftAssignmentManager,
    connected_devices: &[ConnectedDevice],
    active_aircraft: &str,
    _active_profile_name: &str,
    active_profile_path: &str,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    let Some(ref sel_path) = state.selected_profile_path.clone() else {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.label(RichText::new("Select a profile from the left list to view or edit its settings.").color(theme.text_muted));
        });
        return;
    };

    let p_path = Path::new(sel_path);
    let mut profile = match ProfileParser::parse_file(p_path) {
        Ok(p) => p,
        Err(err) => {
            ui.label(RichText::new(format!("Error loading profile at {:?}: {}", p_path, err)).color(theme.danger));
            return;
        }
    };

    let is_active = sel_path == active_profile_path || active_profile_path.ends_with(sel_path);
    let is_fallback = assignments.default_profile.as_deref() == Some(sel_path.as_str());
    let prof_name = profile.name.clone().unwrap_or_else(|| sel_path.clone());

    // 1. HEADER CARD: Profile Name & Core Actions
    ui_card(ui, theme, |ui| {
        // Row 1: Profile Name Heading & Activation Status
        ui.horizontal(|ui| {
            if state.is_renaming {
                ui.label(RichText::new("Rename:").color(theme.text_secondary));
                ui.add(egui::TextEdit::singleline(&mut state.rename_profile_input).desired_width(240.0));
                if ui.button(RichText::new("✔ Save").color(theme.success).strong()).clicked() {
                    let clean = state.rename_profile_input.trim().to_string();
                    if !clean.is_empty() {
                        profile.name = Some(clean.clone());
                        let _ = ProfileParser::save_file(p_path, &profile);
                        state.is_renaming = false;
                        state.status_banner = Some((format!("Renamed profile to '{}'", clean), false));
                    }
                }
                if ui.button("Cancel").clicked() {
                    state.is_renaming = false;
                }
            } else {
                ui.heading(RichText::new(&prof_name).color(theme.text_primary).strong());
                if ui.button(RichText::new("✏ Rename").color(theme.text_secondary).small()).clicked() {
                    state.is_renaming = true;
                    state.rename_profile_input = prof_name.clone();
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if is_active {
                    let active_why = if !active_aircraft.is_empty() && profile.matches_aircraft_title(active_aircraft) {
                        format!("Auto-matched to '{}'", active_aircraft)
                    } else if !active_aircraft.is_empty() && is_fallback {
                        format!("Fallback for '{}'", active_aircraft)
                    } else {
                        "Active (Manual)".to_string()
                    };
                    ui_status_badge(ui, &active_why, true, theme);
                } else if ui
                    .button(RichText::new("⚡ Activate in Simulator").color(theme.success).strong())
                    .on_hover_text("Load and activate this profile now in the hardware engine")
                    .clicked()
                {
                    let _ = cmd_tx.send(GuiCommand::LoadProfile(p_path.to_path_buf()));
                    state.status_banner = Some((format!("Activated profile '{}'", prof_name), false));
                }
            });
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // Row 2: Profile Actions (Fallback, Duplicate, Export, Delete)
        ui.horizontal(|ui| {
            if is_fallback {
                if ui
                    .button(RichText::new("⭐ Generic Fallback (Active)").color(theme.warning).strong().small())
                    .on_hover_text("This profile is loaded when no specific aircraft pattern matches. Click to unset.")
                    .clicked()
                {
                    let _ = cmd_tx.send(GuiCommand::SetDefaultProfile(None));
                    state.status_banner = Some(("Unset default fallback profile".to_string(), false));
                }
            } else if ui
                .button(RichText::new("☆ Set as Generic Fallback").color(theme.text_secondary).small())
                .on_hover_text("Use this profile as the generic fallback when no other aircraft profile matches")
                .clicked()
            {
                let _ = cmd_tx.send(GuiCommand::SetDefaultProfile(Some(sel_path.clone())));
                state.status_banner = Some((format!("Set '{}' as generic fallback profile", prof_name), false));
            }

            if ui
                .button(RichText::new("📋 Duplicate").color(theme.text_secondary).small())
                .on_hover_text("Make a copy of this profile with all device layouts and patterns")
                .clicked()
            {
                let copy_name = format!("{} (Copy)", prof_name);
                let filename: String = copy_name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
                    .collect();
                let dest = PathBuf::from(format!("profiles/{}.xml", filename.trim()));
                let mut dup_prof = profile.clone();
                dup_prof.name = Some(copy_name.clone());
                if ProfileParser::save_file(&dest, &dup_prof).is_ok() {
                    let dest_str = dest.to_string_lossy().to_string();
                    state.selected_profile_path = Some(dest_str);
                    state.status_banner = Some((format!("Duplicated profile as '{}'", copy_name), false));
                }
            }

            if ui
                .button(RichText::new("📤 Export...").color(theme.text_secondary).small())
                .on_hover_text("Export a copy of this profile to any file or folder")
                .clicked()
            {
                if let Some(dest) = rfd::FileDialog::new()
                    .set_file_name(format!("{}.xml", prof_name))
                    .add_filter("Profile XML", &["xml"])
                    .save_file()
                {
                    let _ = fs::copy(p_path, &dest);
                    state.status_banner = Some((format!("Exported profile to {:?}", dest), false));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("🗑 Delete Profile").color(theme.danger).small())
                    .on_hover_text("Delete this local profile file permanently")
                    .clicked()
                {
                    let _ = fs::remove_file(p_path);
                    state.selected_profile_path = None;
                    state.status_banner = Some((format!("Deleted profile '{}'", prof_name), false));
                }
            });
        });
    });

    ui.add_space(6.0);

    // 2. CARD: Aircraft Auto-Switching Patterns
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("AIRCRAFT AUTO-SWITCHING PATTERNS").color(theme.text_secondary).strong().small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pats = profile.aircraft_patterns();
                ui.label(RichText::new(format!("{} pattern(s)", pats.len())).color(theme.text_muted).small());
            });
        });
        ui.label(
            RichText::new("When Microsoft Flight Simulator loads an aircraft matching any pattern below, AeroDeck switches to this profile automatically.")
                .color(theme.text_secondary)
                .small(),
        );

        ui.add_space(4.0);

        // Pattern pills
        let mut remove_pat: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            let patterns = profile.aircraft_patterns();
            if patterns.is_empty() {
                ui.label(RichText::new("No patterns added yet. Add an aircraft title or wildcard pattern below.").color(theme.text_muted).italics().small());
            } else {
                for pat in &patterns {
                    egui::Frame::none()
                        .fill(theme.bg_elevated)
                        .stroke(egui::Stroke::new(1.0_f32, theme.border_subtle))
                        .rounding(4.0)
                        .inner_margin(egui::Vec2::new(8.0, 3.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("✈ {}", pat)).color(theme.text_primary).small());
                                if ui.small_button("✕").on_hover_text("Remove this aircraft pattern").clicked() {
                                    remove_pat = Some(pat.clone());
                                }
                            });
                        });
                }
            }
        });

        if let Some(pat_to_rem) = remove_pat {
            profile.remove_aircraft_pattern(&pat_to_rem);
            let _ = ProfileParser::save_file(p_path, &profile);
            state.status_banner = Some((format!("Removed aircraft pattern '{}'", pat_to_rem), false));
        }

        ui.add_space(6.0);

        // Add pattern controls
        ui.horizontal(|ui| {
            ui.label(RichText::new("Add Pattern:").color(theme.text_secondary).small());
            ui.add(
                egui::TextEdit::singleline(&mut state.new_aircraft_pattern)
                    .hint_text("e.g. PMDG 737* or A320 or C172")
                    .desired_width(200.0),
            );
            if ui.button(RichText::new("➕ Add").color(theme.accent).strong()).clicked() {
                let clean = state.new_aircraft_pattern.trim().to_string();
                if !clean.is_empty() {
                    profile.add_aircraft_pattern(&clean);
                    let _ = ProfileParser::save_file(p_path, &profile);
                    state.new_aircraft_pattern.clear();
                    state.status_banner = Some((format!("Added aircraft pattern '{}'", clean), false));
                }
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // Quick picker from detected sim aircraft
            egui::ComboBox::from_id_salt("quick_add_aircraft_combo")
                .selected_text(RichText::new("➕ Pick from Detected Sim Aircraft...").color(theme.text_secondary).small())
                .width(260.0)
                .show_ui(ui, |ui| {
                    if aircrafts.is_empty() {
                        ui.label(RichText::new("No installed aircraft detected in MSFS folder yet.").color(theme.text_muted).italics());
                    } else {
                        for ac in aircrafts {
                            let label = format!("{} ({})", ac.title, ac.model);
                            if ui.selectable_label(false, label).clicked() {
                                profile.add_aircraft_pattern(&ac.title);
                                let _ = ProfileParser::save_file(p_path, &profile);
                                state.status_banner = Some((format!("Added aircraft pattern '{}'", ac.title), false));
                            }
                        }
                    }
                });
        });
    });

    ui.add_space(6.0);

    // 3. CARD: Controller Layouts in this Profile
    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("CONTROLLER LAYOUTS IN THIS PROFILE").color(theme.text_secondary).strong().small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let dev_count = profile.devices.as_ref().map(|d| d.items.len()).unwrap_or(0);
                ui.label(RichText::new(format!("{} device(s)", dev_count)).color(theme.text_muted).small());
            });
        });
        ui.label(
            RichText::new("Each device has its own layout of buttons, switches, and axes inside this profile. Edited with AeroDeck's own editor.")
                .color(theme.text_secondary)
                .small(),
        );

        ui.add_space(4.0);

        let devices_in_prof = profile
            .devices
            .as_ref()
            .map(|d| d.items.clone())
            .unwrap_or_default();

        // Device chips/tabs
        ui.horizontal_wrapped(|ui| {
            for (idx, dev) in devices_in_prof.iter().enumerate() {
                let is_selected_dev = state.active_device_idx == idx;
                let dev_name = dev.friendly_device_name();
                let mapped = dev.mapped_input_count();

                let chip_builder = |ui: &mut Ui| {
                    ui.horizontal(|ui| {
                        let dot_color = if mapped > 0 { theme.success } else { theme.text_muted };
                        ui.label(RichText::new("●").color(dot_color).small());
                        ui.label(
                            RichText::new(format!("{} ({} mapped)", dev_name, mapped))
                                .color(if is_selected_dev { theme.accent } else { theme.text_primary })
                                .small()
                                .strong(),
                        );
                    });
                };

                let (_, resp) = ui_clickable_card(ui, theme, is_selected_dev, chip_builder);

                if resp.clicked() {
                    state.active_device_idx = idx;
                    state.snippet_browser_open = false;
                    state.confirm_overwrite = None;
                }
            }

            // "Add Controller..." dropdown listing detected hardware
            egui::ComboBox::from_id_salt("add_device_to_profile_combo")
                .selected_text(RichText::new("➕ Add Controller...").color(theme.accent).small().strong())
                .width(220.0)
                .show_ui(ui, |ui| {
                    if connected_devices.is_empty() {
                        ui.label(RichText::new("No hardware controllers detected. Plug in your joystick or yoke.").color(theme.text_muted).italics());
                    } else {
                        for cd in connected_devices {
                            let already_in = devices_in_prof.iter().any(|d| d.matches_vid_pid(cd.vendor_id, cd.product_id));
                            let pfx = if already_in { "✔ " } else { "+ " };
                            let label = format!("{}{}", pfx, cd.name);
                            if ui.selectable_label(already_in, label).clicked() && !already_in {
                                profile.add_device_for_hardware(cd.vendor_id, cd.product_id, &cd.name);
                                let _ = ProfileParser::save_file(p_path, &profile);
                                state.active_device_idx = devices_in_prof.len();
                                state.status_banner = Some((format!("Added device '{}' to profile", cd.name), false));
                            }
                        }
                    }
                });
        });

        ui.add_space(8.0);

        // Selected Device Detail & Actions
        if let Some(selected_dev) = devices_in_prof.get(state.active_device_idx) {
            let (vid, pid) = selected_dev.parsed_vid_pid().unwrap_or((0, 0));
            let dev_name = selected_dev.friendly_device_name();
            let mapped_count = selected_dev.mapped_input_count();

            ui_elevated_card(ui, theme, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Selected: {}", dev_name)).color(theme.text_primary).strong());
                    ui_tag_pill(ui, &format!("0x{:04X}:0x{:04X}", vid, pid), theme.accent, theme);
                    ui_tag_pill(ui, &format!("{} mapped inputs", mapped_count), if mapped_count > 0 { theme.success } else { theme.text_muted }, theme);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Remove device from profile
                        if ui.button(RichText::new("🗑 Remove Controller").color(theme.danger).small()).clicked() {
                            profile.remove_device(vid, pid);
                            let _ = ProfileParser::save_file(p_path, &profile);
                            state.active_device_idx = 0;
                            state.status_banner = Some((format!("Removed controller '{}' from profile", dev_name), false));
                            return;
                        }

                        // Clear layout button
                        if ui.button(RichText::new("🧹 Clear Layout").color(theme.warning).small()).clicked() {
                            let prev = profile.clear_device_layout(vid, pid);
                            let _ = ProfileParser::save_file(p_path, &profile);
                            if let Some(p) = prev {
                                state.undo_last_import = Some(SnippetUndoData {
                                    profile_path: sel_path.clone(),
                                    vendor_id: vid,
                                    product_id: pid,
                                    previous_layout: p,
                                });
                            }
                            state.status_banner = Some((format!("Cleared all bindings for '{}'. You can undo this action.", dev_name), false));
                        }

                        // Undo snippet import
                        if let Some(ref undo) = state.undo_last_import {
                            if undo.profile_path == *sel_path
                                && undo.vendor_id == vid
                                && undo.product_id == pid
                                && ui.button(RichText::new("↩ Undo Snippet Import").color(theme.warning).strong().small()).clicked()
                            {
                                profile.replace_device_layout(vid, pid, undo.previous_layout.clone());
                                let _ = ProfileParser::save_file(p_path, &profile);
                                state.undo_last_import = None;
                                state.status_banner = Some((format!("Restored previous layout for '{}'", dev_name), false));
                            }
                        }

                        // Import snippet button
                        if ui.button(RichText::new("🌐 Import SPAD.neXt Snippet...").color(theme.accent).strong().small()).clicked() {
                            state.snippet_browser_open = !state.snippet_browser_open;
                            if state.snippet_browser_open && state.online_catalog.is_empty() {
                                state.is_fetching_online = true;
                                let _ = cmd_tx.send(GuiCommand::FetchOnlineSpadCatalog {
                                    account: state.spad_account.clone(),
                                });
                            }
                        }
                    });
                });

                // Overwrite Confirmation Banner
                if let Some(ref confirm) = state.confirm_overwrite.clone() {
                    if confirm.vendor_id == vid && confirm.product_id == pid {
                        ui.add_space(8.0);
                        egui::Frame::none()
                            .fill(theme.bg_surface)
                            .stroke(egui::Stroke::new(1.0_f32, theme.warning))
                            .rounding(6.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("⚠").color(theme.warning).strong());
                                    ui.label(
                                        RichText::new(format!(
                                            "This replaces the layout of '{}'. {} existing bindings will be overwritten.",
                                            confirm.device_name, confirm.bindings_count
                                        ))
                                        .color(theme.text_primary),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("Cancel").clicked() {
                                            state.confirm_overwrite = None;
                                        }
                                        if ui.button(RichText::new("✔ Confirm Overwrite").color(theme.danger).strong()).clicked() {
                                            let prev = profile.replace_device_layout(
                                                confirm.vendor_id,
                                                confirm.product_id,
                                                confirm.downloaded_layout.clone(),
                                            );
                                            let _ = ProfileParser::save_file(p_path, &profile);
                                            if let Some(p) = prev {
                                                state.undo_last_import = Some(SnippetUndoData {
                                                    profile_path: sel_path.clone(),
                                                    vendor_id: confirm.vendor_id,
                                                    product_id: confirm.product_id,
                                                    previous_layout: p,
                                                });
                                            }
                                            state.confirm_overwrite = None;
                                            state.snippet_browser_open = false;
                                            state.status_banner = Some((
                                                format!("Imported snippet layout for '{}'. Click 'Undo' if you want to revert.", confirm.device_name),
                                                false,
                                            ));
                                        }
                                    });
                                });
                            });
                    }
                }

                // SPAD.neXt Snippet Browser & Importer Drawer
                if state.snippet_browser_open {
                    ui.add_space(8.0);
                    render_snippet_browser(
                        ui,
                        state,
                        &mut profile,
                        p_path,
                        sel_path,
                        vid,
                        pid,
                        &dev_name,
                        mapped_count,
                        cmd_tx,
                        theme,
                    );
                }
            });
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn render_snippet_browser(
    ui: &mut Ui,
    state: &mut AircraftProfilesViewState,
    profile: &mut SpadProfile,
    p_path: &Path,
    sel_path: &str,
    target_vid: u16,
    target_pid: u16,
    target_dev_name: &str,
    target_mapped_count: usize,
    cmd_tx: &mpsc::UnboundedSender<GuiCommand>,
    theme: &AeroTheme,
) {
    egui::Frame::none()
        .fill(theme.bg_surface)
        .stroke(egui::Stroke::new(1.0_f32, theme.accent))
        .rounding(6.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🌐 Import SPAD.neXt Snippet (Ready-Made Device Layout)").color(theme.accent).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕ Close").clicked() {
                        state.snippet_browser_open = false;
                    }
                    if ui.button(RichText::new("📂 Import from local .xml...").color(theme.text_secondary)).clicked() {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter("Snippet XML", &["xml", "spadprofile"])
                            .pick_file()
                        {
                            if let Ok(incoming) = ProfileParser::parse_file(&file) {
                                apply_snippet_to_device(
                                    state,
                                    profile,
                                    p_path,
                                    sel_path,
                                    target_vid,
                                    target_pid,
                                    target_dev_name,
                                    target_mapped_count,
                                    &incoming,
                                );
                            }
                        }
                    }
                });
            });

            ui.label(
                RichText::new(format!(
                    "Importing a snippet replaces ONLY the layout of '{}'. It is a one-time copy into your profile.",
                    target_dev_name
                ))
                .color(theme.text_secondary)
                .small(),
            );

            ui.add_space(4.0);

            // Search controls
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.snippet_search)
                        .hint_text("Search snippets by aircraft or name...")
                        .desired_width(240.0),
                );
                ui.checkbox(
                    &mut state.online_match_my_devices,
                    RichText::new("Match this controller VID:PID").color(theme.text_primary).small(),
                );
                if ui.button("🔄 Refresh Catalog").clicked() {
                    state.is_fetching_online = true;
                    let _ = cmd_tx.send(GuiCommand::FetchOnlineSpadCatalog {
                        account: state.spad_account.clone(),
                    });
                }
            });

            ui.add_space(6.0);

            let target_vp = format!("{:04X}:{:04X}", target_vid, target_pid);
            let s_lower = state.snippet_search.trim().to_lowercase();

            let mut to_download: Option<(i32, String)> = None;

            let matches: Vec<(i32, String, String, String, u32, i32)> = state
                .online_catalog
                .iter()
                .filter(|entry| {
                    if state.online_match_my_devices && !entry.devices.iter().any(|d| d.eq_ignore_ascii_case(&target_vp)) {
                        return false;
                    }
                    if !s_lower.is_empty() {
                        let in_name = entry.name.to_lowercase().contains(&s_lower);
                        let in_ac = entry.aircrafts.iter().any(|a| a.to_lowercase().contains(&s_lower));
                        let in_author = entry.author.to_lowercase().contains(&s_lower);
                        if !in_name && !in_ac && !in_author {
                            return false;
                        }
                    }
                    true
                })
                .map(|e| (e.snippet_id, e.name.clone(), e.author.clone(), e.version.clone(), e.dlcount, e.rating))
                .collect();

            if state.is_fetching_online && state.online_catalog.is_empty() {
                ui.label(RichText::new("⏳ Loading community snippets from update.spadnext.net...").color(theme.accent).italics());
            } else if matches.is_empty() {
                ui.label(RichText::new("No snippets match your filter. Try unchecking 'Match this controller' or clear search.").color(theme.text_muted).italics());
            } else {
                ScrollArea::vertical()
                    .id_salt("snippet_catalog_scroll")
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for (snip_id, name, author, version, dlcount, rating) in matches.iter().take(40) {
                            ui_elevated_card(ui, theme, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(name).color(theme.accent).strong());
                                    ui.label(RichText::new(format!("by {} • v{}", author, version)).color(theme.text_secondary).small());
                                    ui.label(RichText::new(format!("⬇ {} dl • ★ {}", dlcount, rating)).color(theme.warning).small());

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button(RichText::new("📥 Import this Layout").color(theme.success).strong()).clicked() {
                                            to_download = Some((*snip_id, name.clone()));
                                        }
                                    });
                                });
                            });
                            ui.add_space(2.0);
                        }
                    });
            }

            if let Some((snip_id, name)) = to_download {
                match ProfileRepository::download_online_spad_snippet(
                    snip_id,
                    &name,
                    &state.spad_account,
                ) {
                    Ok(downloaded_file) => {
                        if let Ok(incoming) = ProfileParser::parse_file(&downloaded_file) {
                            apply_snippet_to_device(
                                state,
                                profile,
                                p_path,
                                sel_path,
                                target_vid,
                                target_pid,
                                target_dev_name,
                                target_mapped_count,
                                &incoming,
                            );
                        }
                    }
                    Err(err) => {
                        state.status_banner = Some((format!("Failed to download snippet: {}", err), true));
                    }
                }
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn apply_snippet_to_device(
    state: &mut AircraftProfilesViewState,
    profile: &mut SpadProfile,
    p_path: &Path,
    sel_path: &str,
    target_vid: u16,
    target_pid: u16,
    target_dev_name: &str,
    target_mapped_count: usize,
    incoming: &SpadProfile,
) {
    // Find matching device in incoming or take first device with actions
    let inc_dev = incoming
        .devices
        .as_ref()
        .and_then(|d| {
            d.items
                .iter()
                .find(|dev| dev.matches_vid_pid(target_vid, target_pid) && dev.total_action_count() > 0)
                .or_else(|| d.items.iter().find(|dev| dev.total_action_count() > 0))
                .or_else(|| d.items.first())
        })
        .cloned();

    let Some(mut new_layout) = inc_dev else {
        state.status_banner = Some(("Downloaded snippet contained no device mappings".to_string(), true));
        return;
    };

    new_layout.name = Some(target_dev_name.to_string());
    new_layout.vendor_id = Some(format!("0x{:04X}", target_vid));
    new_layout.product_id = Some(format!("0x{:04X}", target_pid));

    if target_mapped_count > 0 {
        // Show confirmation before overwriting
        state.confirm_overwrite = Some(SnippetOverwriteConfirm {
            device_name: target_dev_name.to_string(),
            vendor_id: target_vid,
            product_id: target_pid,
            bindings_count: target_mapped_count,
            downloaded_layout: new_layout,
            profile_path: sel_path.to_string(),
        });
    } else {
        // Direct import without asking
        let prev = profile.replace_device_layout(target_vid, target_pid, new_layout);
        let _ = ProfileParser::save_file(p_path, profile);
        if let Some(p) = prev {
            state.undo_last_import = Some(SnippetUndoData {
                profile_path: sel_path.to_string(),
                vendor_id: target_vid,
                product_id: target_pid,
                previous_layout: p,
            });
        }
        state.snippet_browser_open = false;
        state.status_banner = Some((
            format!("Imported snippet layout for '{}'. Click 'Undo' if you want to revert.", target_dev_name),
            false,
        ));
    }
}
