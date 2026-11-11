//! Profiles page: local profiles, aircraft patterns, controller layouts and SPAD.neXt snippets.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::ComponentHandle;

use super::*;
use crate::gui::editing as ed;
use crate::profile::schema::DeviceProfile;
use crate::profile::{OnlineSpadProfileEntry, SpadAccountConfig};

/// Parsed summary of a local profile file.
pub(crate) struct ProfileMeta {
    pub path: String,
    pub name: String,
    pub patterns: Vec<String>,
    pub devices: usize,
    pub inputs: usize,
}

pub(crate) struct ProfilesUi {
    pub selected: Option<String>,
    pub detail: Option<SpadProfile>,
    pub layout_idx: usize,
    pub picker_selected: BTreeSet<String>,
    pub undo_clear: Option<(String, u16, u16, DeviceProfile)>,
    pub undo_snippet: Option<(String, SpadProfile)>,
    pub account: SpadAccountConfig,
    pub catalog: Vec<OnlineSpadProfileEntry>,
    pub fetching: bool,
    pub importing: bool,
    pub cache: Vec<ProfileMeta>,
}

impl ProfilesUi {
    pub fn new() -> Self {
        Self {
            selected: None,
            detail: None,
            layout_idx: 0,
            picker_selected: BTreeSet::new(),
            undo_clear: None,
            undo_snippet: None,
            account: SpadAccountConfig::load_or_autodiscover(),
            catalog: Vec::new(),
            fetching: false,
            importing: false,
            cache: Vec::new(),
        }
    }
}

fn read_meta(path: &str) -> ProfileMeta {
    let parsed = ProfileParser::parse_file(Path::new(path)).ok();
    let name = parsed.as_ref().and_then(|p| p.name.clone()).unwrap_or_else(|| file_stem(path).replace('_', " "));
    let patterns = parsed.as_ref().map(|p| p.aircraft_patterns()).unwrap_or_default();
    let (devices, inputs) = parsed
        .as_ref()
        .and_then(|p| p.devices.as_ref())
        .map(|d| (d.items.len(), d.items.iter().map(|x| x.mapped_input_count()).sum()))
        .unwrap_or((0, 0));
    ProfileMeta { path: path.to_string(), name, patterns, devices, inputs }
}

impl App {
    fn reload_profile_cache(&self) {
        let paths = ProfileRepository::list_local_profiles();
        let cache: Vec<ProfileMeta> = paths.iter().map(|p| read_meta(p)).collect();
        let mut st = self.st.borrow_mut();
        st.local_profiles = paths;
        st.p.cache = cache;
    }

    pub(crate) fn refresh_profiles(&self) {
        self.reload_profile_cache();
        {
            let mut st = self.st.borrow_mut();
            let active = st.profile_path.clone();
            // Default selection: the active profile
            let valid = st.p.selected.as_ref().is_some_and(|s| st.p.cache.iter().any(|m| &m.path == s));
            if !valid {
                st.p.selected = st
                    .p
                    .cache
                    .iter()
                    .find(|m| same_profile_path(&m.path, &active))
                    .or(st.p.cache.first())
                    .map(|m| m.path.clone());
            }
            st.p.detail = st.p.selected.as_ref().and_then(|p| ProfileParser::parse_file(Path::new(p)).ok());
        }
        self.refresh_profile_list();
        self.refresh_profile_detail();
        self.refresh_shell();
    }

    fn refresh_profile_list(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ps = ui.global::<ProfilesState>();
        let q = ps.get_search().trim().to_lowercase();
        let st = self.st.borrow();
        let fallback = st.assignments.default_profile.clone();
        let items: Vec<ProfileItem> = st
            .p
            .cache
            .iter()
            .filter(|m| {
                q.is_empty()
                    || m.path.to_lowercase().contains(&q)
                    || m.name.to_lowercase().contains(&q)
                    || m.patterns.iter().any(|p| p.to_lowercase().contains(&q))
            })
            .map(|m| {
                let is_fallback = fallback.as_deref().is_some_and(|f| same_profile_path(f, &m.path));
                ProfileItem {
                    path: ss(&m.path),
                    name: ss(&m.name),
                    patterns: ss(if !m.patterns.is_empty() {
                        format!("Matches {}", m.patterns.join(", "))
                    } else if is_fallback {
                        "Fallback for any aircraft".to_string()
                    } else {
                        "No aircraft patterns".to_string()
                    }),
                    devices: m.devices as i32,
                    inputs: m.inputs as i32,
                    active: same_profile_path(&m.path, &st.profile_path),
                    fallback: is_fallback,
                }
            })
            .collect();
        ps.set_profiles(model(items));
        ps.set_selected_path(ss(st.p.selected.clone().unwrap_or_default()));
        ps.set_auto_switch(st.assignments.auto_switch_enabled);
    }

    fn refresh_profile_detail(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ps = ui.global::<ProfilesState>();
        let st = self.st.borrow();
        let (Some(path), Some(prof)) = (st.p.selected.as_ref(), st.p.detail.as_ref()) else {
            ps.set_has_selection(false);
            return;
        };
        ps.set_has_selection(true);
        let name = prof.name.clone().unwrap_or_else(|| file_stem(path));
        ps.set_name(ss(&name));
        ps.set_file(ss(path));
        let is_active = same_profile_path(path, &st.profile_path);
        let is_fallback = st.assignments.default_profile.as_deref().is_some_and(|f| same_profile_path(f, path));
        ps.set_is_active(is_active);
        ps.set_is_fallback(is_fallback);
        let ac = &st.aircraft;
        ps.set_active_why(ss(if ac != NO_AIRCRAFT && prof.matches_aircraft_title(ac) {
            format!("Active · matches {}", ac)
        } else if ac != NO_AIRCRAFT && is_fallback {
            format!("Active · fallback for {}", ac)
        } else {
            "Active".to_string()
        }));
        let patterns = prof.aircraft_patterns();
        ps.set_patterns(strings(&patterns));

        // Aircraft picker
        let q = ps.get_picker_query().to_lowercase();
        let visible: Vec<AircraftItem> = st
            .aircrafts
            .iter()
            .filter(|a| q.is_empty() || a.title.to_lowercase().contains(&q))
            .map(|a| AircraftItem {
                title: ss(&a.title),
                model: ss(&a.model),
                checked: st.p.picker_selected.contains(&a.title),
                already: patterns.iter().any(|p| p.eq_ignore_ascii_case(&a.title)),
            })
            .collect();
        ps.set_picker_visible(visible.iter().filter(|a| !a.already).count() as i32);
        ps.set_picker_items(model(visible));
        ps.set_picker_count(st.p.picker_selected.len() as i32);

        // Controller layouts
        let devs: Vec<&DeviceProfile> = prof.devices.as_ref().map(|d| d.items.iter().collect()).unwrap_or_default();
        let idx = st.p.layout_idx.min(devs.len().saturating_sub(1));
        ps.set_layout_devices(model(
            devs.iter()
                .map(|d| {
                    let (v, p) = d.parsed_vid_pid().unwrap_or((0, 0));
                    LayoutDevice { name: ss(d.friendly_device_name()), vidpid: ss(format!("{:04X}:{:04X}", v, p)), mapped: d.mapped_input_count() as i32 }
                })
                .collect(),
        ));
        ps.set_layout_device_names(strings(devs.iter().map(|d| {
            let n = d.friendly_device_name();
            super::short_device(&n)
        })));
        ps.set_layout_index(idx as i32);
        ps.set_addable_devices(strings(st.devices.iter().map(|cd| {
            let already = devs.iter().any(|d| d.matches_vid_pid(cd.vendor_id, cd.product_id));
            format!("{}{}", cd.name, if already { "  · in profile" } else { "" })
        })));
        match devs.get(idx) {
            Some(d) => {
                let (v, p) = d.parsed_vid_pid().unwrap_or((0, 0));
                ps.set_has_layout(true);
                ps.set_layout_name(ss(d.friendly_device_name()));
                ps.set_layout_vidpid(ss(format!("{:04X}:{:04X}", v, p)));
                ps.set_layout_mapped(d.mapped_input_count() as i32);
                ps.set_can_undo_clear(st.p.undo_clear.as_ref().is_some_and(|u| &u.0 == path && u.1 == v && u.2 == p));
            }
            None => ps.set_has_layout(false),
        }
        ps.set_can_undo_snippet(st.p.undo_snippet.as_ref().is_some_and(|(p, _)| p == path));
        drop(st);
        self.refresh_snippets();
    }

    fn refresh_snippets(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ps = ui.global::<ProfilesState>();
        let st = self.st.borrow();
        ps.set_fetching(st.p.fetching);
        ps.set_importing(st.p.importing);
        let target = st.p.detail.as_ref().and_then(|p| p.devices.as_ref()).and_then(|d| d.items.get(st.p.layout_idx)).and_then(|d| d.parsed_vid_pid());
        let target_vp = target.map(|(v, p)| format!("{:04X}:{:04X}", v, p)).unwrap_or_default();
        let q = ps.get_snippet_search().trim().to_lowercase();
        let only_this = ps.get_snippet_match();
        let items: Vec<SnippetItem> = st
            .p
            .catalog
            .iter()
            .filter(|e| !only_this || e.devices.iter().any(|d| d.eq_ignore_ascii_case(&target_vp)))
            .filter(|e| {
                q.is_empty() || e.name.to_lowercase().contains(&q) || e.author.to_lowercase().contains(&q) || e.aircrafts.iter().any(|a| a.to_lowercase().contains(&q))
            })
            .take(200)
            .map(|e| SnippetItem {
                id: e.snippet_id,
                name: ss(&e.name),
                author: ss(&e.author),
                version: ss(&e.version),
                downloads: e.dlcount as i32,
                rating: e.rating,
                aircraft: ss(e.aircrafts.iter().take(2).cloned().collect::<Vec<_>>().join(", ")),
            })
            .collect();
        ps.set_snippets_empty(ss(if st.p.catalog.is_empty() {
            "The catalog is empty. Click Refresh to load it."
        } else {
            "No snippets match. Untick 'Only this controller' or clear the search."
        }));
        ps.set_snippets(model(items));
    }

    /// Edits the selected profile file; reloads it in the engine if it is the active one.
    fn edit_selected(self: &Rc<Self>, message: impl Into<String>, f: impl FnOnce(&mut SpadProfile)) {
        let (path, active) = {
            let mut st = self.st.borrow_mut();
            let Some(path) = st.p.selected.clone() else { return };
            let Some(prof) = st.p.detail.as_mut() else { return };
            f(prof);
            if let Err(e) = ProfileParser::save_file(Path::new(&path), prof) {
                drop(st);
                self.toast(format!("Could not save: {}", e), 4);
                return;
            }
            let active = same_profile_path(&path, &st.profile_path);
            (path, active)
        };
        if active {
            self.send(GuiCommand::LoadProfile(PathBuf::from(&path)));
        }
        self.toast(message, 2);
        self.refresh_profiles();
    }

    fn selected_layout_vidpid(&self) -> Option<(u16, u16, String)> {
        let st = self.st.borrow();
        let d = st.p.detail.as_ref()?.devices.as_ref()?.items.get(st.p.layout_idx)?;
        let (v, p) = d.parsed_vid_pid()?;
        Some((v, p, d.friendly_device_name()))
    }

    fn apply_snippet(self: &Rc<Self>, incoming: SpadProfile) {
        let has_mappings = incoming.devices.as_ref().is_some_and(|d| d.items.iter().any(|dev| dev.total_action_count() > 0));
        if !has_mappings {
            self.toast("That SPAD.neXt profile contains no mappings", 3);
            return;
        }
        let target = self.selected_layout_vidpid();
        let before = self.st.borrow().p.detail.clone();
        let sel = self.st.borrow().p.selected.clone();
        let mut summary_text = String::new();
        self.edit_selected("", |prof| {
            // Every device in the snippet merges into the matching controller (by VID:PID);
            // only the inputs the snippet defines are replaced.
            let summary = prof.merge_from_profile(&incoming, None, false);
            if let Some((v, p, name)) = &target {
                let matches = incoming.devices.as_ref().is_some_and(|d| d.items.iter().any(|dev| dev.matches_vid_pid(*v, *p)));
                if !matches && *v != 0 {
                    // A snippet made for another model: map it onto the selected controller.
                    let id = format!("DEV_{:04X}_{:04X}", v, p);
                    prof.merge_from_profile(&incoming, Some((*v, *p, &id, name)), false);
                }
            }
            summary_text = format!("Added {} mappings ({})", summary.total_events_merged, summary.merged_device_names.join(", "));
        });
        if let (Some(b), Some(s)) = (before, sel) {
            self.st.borrow_mut().p.undo_snippet = Some((s, b));
        }
        self.toast(summary_text, 2);
        self.refresh_profile_detail();
    }

    pub(crate) fn on_snippet_downloaded(self: &Rc<Self>, profile_path: String, result: std::result::Result<PathBuf, String>) {
        self.st.borrow_mut().p.importing = false;
        let selected = self.st.borrow().p.selected.clone();
        match result {
            Ok(file) if selected.as_deref() == Some(profile_path.as_str()) => match ProfileParser::parse_file(&file) {
                Ok(incoming) => self.apply_snippet(incoming),
                Err(e) => self.toast(format!("Could not read the snippet: {}", e), 4),
            },
            Ok(_) => self.toast("Snippet downloaded, but another profile is selected now", 3),
            Err(e) => self.toast(format!("Download failed: {}", e), 4),
        }
        self.refresh_snippets();
    }
}

pub(crate) fn wire(app: &Rc<App>, ui: &AppWindow) {
    let ps = ui.global::<ProfilesState>();

    let a = app.clone();
    ps.on_set_auto_switch(move |v| {
        a.send(GuiCommand::SetAutoSwitchProfile(v));
        a.st.borrow_mut().assignments.auto_switch_enabled = v;
        a.toast(if v { "Profiles follow the aircraft" } else { "Auto-switch off" }, 2);
    });
    let a = app.clone();
    ps.on_import_profile(move || {
        let Some(file) = rfd::FileDialog::new().add_filter("Profile XML", &["xml", "spadprofile"]).pick_file() else { return };
        let Some(name) = file.file_name() else { return };
        let _ = std::fs::create_dir_all("profiles");
        let dest = Path::new("profiles").join(name);
        match std::fs::copy(&file, &dest) {
            Ok(_) => {
                a.st.borrow_mut().p.selected = Some(dest.to_string_lossy().to_string());
                a.send(GuiCommand::LoadProfile(dest));
                a.toast(format!("Imported {}", name.to_string_lossy()), 2);
                a.refresh_profiles();
            }
            Err(e) => a.toast(format!("Import failed: {}", e), 4),
        }
    });
    let a = app.clone();
    ps.on_create(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let ps = ui.global::<ProfilesState>();
        let name = ps.get_new_name().trim().to_string();
        if name.is_empty() {
            return;
        }
        let dest = ed::profile_file_for(&name);
        let mut prof = SpadProfile {
            version: Some(2),
            is_stock_profile: Some(false),
            name: Some(name.clone()),
            variables: None,
            options: None,
            assigned_aircrafts: None,
            devices: Some(crate::profile::schema::DeviceList { items: Vec::new() }),
            profile_info: None,
        };
        for d in &a.st.borrow().devices {
            prof.add_device_for_hardware(d.vendor_id, d.product_id, &d.name);
        }
        let _ = std::fs::create_dir_all("profiles");
        match ProfileParser::save_file(&dest, &prof) {
            Ok(_) => {
                ps.set_creating(false);
                a.st.borrow_mut().p.selected = Some(dest.to_string_lossy().to_string());
                a.send(GuiCommand::LoadProfile(dest));
                a.toast(format!("Created '{}'", name), 2);
                a.refresh_profiles();
            }
            Err(e) => a.toast(format!("Could not create the profile: {}", e), 4),
        }
    });
    let a = app.clone();
    ps.on_search_changed(move || a.refresh_profile_list());
    let a = app.clone();
    ps.on_select(move |path| {
        {
            let mut st = a.st.borrow_mut();
            st.p.selected = Some(path.to_string());
            st.p.layout_idx = 0;
            st.p.picker_selected.clear();
        }
        if let Some(ui) = a.ui.upgrade() {
            let ps = ui.global::<ProfilesState>();
            ps.set_renaming(false);
            ps.set_confirm_delete(false);
            ps.set_snippets_open(false);
            ps.set_picker_open(false);
        }
        a.refresh_profiles();
    });
    let a = app.clone();
    ps.on_activate(move |path| {
        a.st.borrow_mut().auto_matched = false;
        a.send(GuiCommand::LoadProfile(PathBuf::from(path.as_str())));
        a.toast(format!("Activated {}", file_stem(&path)), 2);
    });
    let a = app.clone();
    ps.on_start_rename(move || {
        if let Some(ui) = a.ui.upgrade() {
            let ps = ui.global::<ProfilesState>();
            ps.set_rename_text(ps.get_name());
            ps.set_renaming(true);
        }
    });
    let a = app.clone();
    ps.on_rename(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let ps = ui.global::<ProfilesState>();
        let name = ps.get_rename_text().trim().to_string();
        if name.is_empty() {
            return;
        }
        ps.set_renaming(false);
        a.edit_selected(format!("Renamed to '{}'", name), |p| p.name = Some(name.clone()));
    });
    let a = app.clone();
    ps.on_toggle_fallback(move || {
        let (sel, is_fallback) = {
            let st = a.st.borrow();
            let sel = st.p.selected.clone();
            let fb = sel.as_ref().is_some_and(|s| st.assignments.default_profile.as_deref().is_some_and(|f| same_profile_path(f, s)));
            (sel, fb)
        };
        let Some(sel) = sel else { return };
        if is_fallback {
            a.send(GuiCommand::SetDefaultProfile(None));
            a.st.borrow_mut().assignments.default_profile = None;
            a.toast("No generic fallback profile", 2);
        } else {
            a.send(GuiCommand::SetDefaultProfile(Some(sel.clone())));
            a.st.borrow_mut().assignments.default_profile = Some(sel.clone());
            a.toast(format!("{} is the fallback for unknown aircraft", file_stem(&sel)), 2);
        }
        a.refresh_profiles();
    });
    let a = app.clone();
    ps.on_duplicate(move || {
        let Some(mut prof) = a.st.borrow().p.detail.clone() else { return };
        let name = format!("{} (Copy)", prof.name.clone().unwrap_or_else(|| "Profile".to_string()));
        prof.name = Some(name.clone());
        let dest = ed::profile_file_for(&name);
        match ProfileParser::save_file(&dest, &prof) {
            Ok(_) => {
                a.st.borrow_mut().p.selected = Some(dest.to_string_lossy().to_string());
                a.toast(format!("Duplicated as '{}'", name), 2);
                a.refresh_profiles();
            }
            Err(e) => a.toast(format!("Could not duplicate: {}", e), 4),
        }
    });
    let a = app.clone();
    ps.on_export(move || {
        let (Some(path), name) = ({
            let st = a.st.borrow();
            (st.p.selected.clone(), st.p.detail.as_ref().and_then(|p| p.name.clone()).unwrap_or_else(|| "profile".to_string()))
        }) else {
            return;
        };
        if let Some(dest) = rfd::FileDialog::new().set_file_name(format!("{}.xml", name)).add_filter("Profile XML", &["xml"]).save_file() {
            match std::fs::copy(&path, &dest) {
                Ok(_) => a.toast(format!("Exported to {}", dest.display()), 2),
                Err(e) => a.toast(format!("Export failed: {}", e), 4),
            }
        }
    });
    let a = app.clone();
    ps.on_delete(move || {
        let Some(path) = a.st.borrow().p.selected.clone() else { return };
        match std::fs::remove_file(&path) {
            Ok(_) => {
                a.st.borrow_mut().p.selected = None;
                a.toast(format!("Deleted {}", file_stem(&path)), 2);
            }
            Err(e) => a.toast(format!("Could not delete: {}", e), 4),
        }
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<ProfilesState>().set_confirm_delete(false);
        }
        a.refresh_profiles();
    });

    // Patterns
    let a = app.clone();
    ps.on_remove_pattern(move |pat| {
        let pat = pat.to_string();
        a.edit_selected(format!("Removed '{}'", pat), |p| p.remove_aircraft_pattern(&pat));
    });
    let a = app.clone();
    ps.on_add_pattern(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let ps = ui.global::<ProfilesState>();
        let pat = ps.get_new_pattern().trim().to_string();
        if pat.is_empty() {
            return;
        }
        ps.set_new_pattern(SharedString::new());
        a.edit_selected(format!("Added '{}'", pat), |p| p.add_aircraft_pattern(&pat));
    });
    let a = app.clone();
    ps.on_toggle_picker(move || {
        if let Some(ui) = a.ui.upgrade() {
            let ps = ui.global::<ProfilesState>();
            ps.set_picker_open(!ps.get_picker_open());
        }
        a.st.borrow_mut().p.picker_selected.clear();
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_picker_filter_changed(move || a.refresh_profile_detail());
    let a = app.clone();
    ps.on_picker_toggle(move |title| {
        {
            let mut st = a.st.borrow_mut();
            let t = title.to_string();
            if !st.p.picker_selected.remove(&t) {
                st.p.picker_selected.insert(t);
            }
        }
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_picker_select_all(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let q = ui.global::<ProfilesState>().get_picker_query().to_lowercase();
        {
            let mut st = a.st.borrow_mut();
            let existing = st.p.detail.as_ref().map(|p| p.aircraft_patterns()).unwrap_or_default();
            let titles: Vec<String> = st
                .aircrafts
                .iter()
                .map(|x| x.title.clone())
                .filter(|t| q.is_empty() || t.to_lowercase().contains(&q))
                .filter(|t| !existing.iter().any(|e| e.eq_ignore_ascii_case(t)))
                .collect();
            st.p.picker_selected.extend(titles);
        }
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_picker_clear(move || {
        a.st.borrow_mut().p.picker_selected.clear();
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_picker_add(move || {
        let (titles, sel) = {
            let st = a.st.borrow();
            (st.p.picker_selected.iter().cloned().collect::<Vec<_>>(), st.p.selected.clone())
        };
        let Some(sel) = sel else { return };
        let n = titles.len();
        let t2 = titles.clone();
        a.edit_selected(format!("Added {} aircraft", n), |p| {
            for t in &t2 {
                p.add_aircraft_pattern(t);
            }
        });
        a.send(GuiCommand::BindAircraftsToProfile { aircraft_titles: titles, profile_path: sel });
        a.st.borrow_mut().p.picker_selected.clear();
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<ProfilesState>().set_picker_open(false);
        }
        a.refresh_profile_detail();
    });

    // Controller layouts
    let a = app.clone();
    ps.on_select_layout(move |i| {
        a.st.borrow_mut().p.layout_idx = i.max(0) as usize;
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_add_device(move |i| {
        let Some(cd) = a.st.borrow().devices.get(i.max(0) as usize).cloned() else { return };
        let already = a
            .st
            .borrow()
            .p
            .detail
            .as_ref()
            .and_then(|p| p.devices.as_ref())
            .is_some_and(|d| d.items.iter().any(|x| x.matches_vid_pid(cd.vendor_id, cd.product_id)));
        if already {
            a.toast(format!("{} is already in this profile", cd.name), 3);
            return;
        }
        a.edit_selected(format!("Added {}", cd.name), |p| p.add_device_for_hardware(cd.vendor_id, cd.product_id, &cd.name));
        let n = a.st.borrow().p.detail.as_ref().and_then(|p| p.devices.as_ref()).map(|d| d.items.len()).unwrap_or(1);
        a.st.borrow_mut().p.layout_idx = n.saturating_sub(1);
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_remove_device(move || {
        let Some((v, p, name)) = a.selected_layout_vidpid() else { return };
        a.edit_selected(format!("Removed {}", name), |prof| {
            prof.remove_device(v, p);
        });
        a.st.borrow_mut().p.layout_idx = 0;
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_clear_layout(move || {
        let Some((v, p, name)) = a.selected_layout_vidpid() else { return };
        let mut prev = None;
        a.edit_selected(format!("Cleared {}: you can undo", name), |prof| prev = prof.clear_device_layout(v, p));
        if let (Some(prev), Some(sel)) = (prev, a.st.borrow().p.selected.clone()) {
            a.st.borrow_mut().p.undo_clear = Some((sel, v, p, prev));
        }
        a.refresh_profile_detail();
    });
    let a = app.clone();
    ps.on_undo_clear(move || {
        let Some((_, v, p, layout)) = a.st.borrow_mut().p.undo_clear.take() else { return };
        a.edit_selected("Layout restored", |prof| {
            prof.replace_device_layout(v, p, layout);
        });
    });

    // Snippets
    let a = app.clone();
    ps.on_toggle_snippets(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let ps = ui.global::<ProfilesState>();
        let open = !ps.get_snippets_open();
        ps.set_snippets_open(open);
        let need = open && a.st.borrow().p.catalog.is_empty() && !a.st.borrow().p.fetching;
        if need {
            a.st.borrow_mut().p.fetching = true;
            let account = a.st.borrow().p.account.clone();
            a.send(GuiCommand::FetchOnlineSpadCatalog { account });
        }
        a.refresh_snippets();
    });
    let a = app.clone();
    ps.on_snippet_filter_changed(move || a.refresh_snippets());
    let a = app.clone();
    ps.on_refresh_catalog(move || {
        a.st.borrow_mut().p.fetching = true;
        let account = a.st.borrow().p.account.clone();
        a.send(GuiCommand::FetchOnlineSpadCatalog { account });
        a.refresh_snippets();
    });
    let a = app.clone();
    ps.on_import_snippet(move |id| {
        let (name, account, sel) = {
            let st = a.st.borrow();
            let name = st.p.catalog.iter().find(|e| e.snippet_id == id).map(|e| e.name.clone()).unwrap_or_default();
            (name, st.p.account.clone(), st.p.selected.clone())
        };
        let Some(sel) = sel else { return };
        a.st.borrow_mut().p.importing = true;
        a.refresh_snippets();
        // Downloading blocks on the network: do it off the UI thread.
        let tx = a.task_tx.clone();
        std::thread::spawn(move || {
            let result = ProfileRepository::download_online_spad_snippet(id, &name, &account).map_err(|e| e.to_string());
            let _ = tx.send(Task::SnippetDownloaded { profile_path: sel, result });
        });
    });
    let a = app.clone();
    ps.on_import_local_snippet(move || {
        let Some(file) = rfd::FileDialog::new().add_filter("Snippet XML", &["xml", "spadprofile"]).pick_file() else { return };
        match ProfileParser::parse_file(&file) {
            Ok(incoming) => a.apply_snippet(incoming),
            Err(e) => a.toast(format!("Could not read {}: {}", file.display(), e), 4),
        }
    });
    let a = app.clone();
    ps.on_undo_snippet(move || {
        let Some((_, prev)) = a.st.borrow_mut().p.undo_snippet.take() else { return };
        a.edit_selected("Undid the last snippet import", |p| *p = prev);
    });
}
