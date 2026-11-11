# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-11-11

### Changed
- New native interface built with Slint, replacing egui: sidebar navigation, an Overview page, a glass-cockpit dark theme and a light theme (remembered), bundled Inter and JetBrains Mono fonts.
- New application icon (stylised attitude indicator), regenerated in every size, `.ico` and hicolor theme.
- "Follow hardware" in Controls reacts to real axis moves (3 % of travel) instead of jitter.
- Snippet downloads run in the background instead of freezing the window.

### Added
- Window icon on Wayland when AeroDeck is not installed (e.g. `cargo run`): the GUI writes a hidden `aerodeck.desktop` and the hicolor icons into `~/.local/share`, unless a system or user installation already provides them.
- Reverse gate on "Throttle + reverse" levers (`REVERSEGATE`, default 20 % of the travel past the idle detent): resting a hand on the lever no longer deploys the reversers; reverse engages beyond the gate and releases a few percent before it.
- Linux packages for every release: `.deb` (Debian / Ubuntu), `.rpm` (Fedora / openSUSE), a `PKGBUILD` (Arch / CachyOS / Manjaro), an AppImage and a portable tarball with `install.sh`.
- GitHub Actions: CI (build, clippy, tests) on every push and pull request; tagging `vX.Y.Z` builds the packages and publishes the GitHub Release.
- Data directory: outside a folder that already holds AeroDeck data, the GUI keeps profiles, aircraft assignments and the SPAD.neXt account in `~/.local/share/aerodeck` (`AERODECK_HOME` overrides).

### Removed
- Virtual Saitek panel page and the in-app UI scale buttons (the desktop scale factor applies; `SLINT_SCALE_FACTOR` overrides it).

### Fixed
- RPN templates inserted their description instead of the code.

## [0.2.0] - 2026-11-11

### Added
- Thrust reverse on throttle axes: the "Throttle + reverse" lever sends reverse thrust past the idle detent (full reverse in the Reverse zone or at the end of travel).
- Axis trigger zones (Zone enter / Zone leave) with hysteresis and an optional deadzone that freezes the axis around the zone, e.g. spoiler ARM at the bottom of the speedbrake lever.
- Trigger zones editor under each axis; Zone enter / Zone leave triggers in the Add Action form.
- SimVar subscriptions, writes and conditions.
- PMDG SDK support: event IDs and data layout read from the installed SDK headers; PMDG events, cockpit data and data broadcast (CLI and GUI).
- SPAD.neXt-compatible input names (BUTTON_n, HATSWITCH_n with POV directions) and PRESSSHORT / PRESSLONG / HELD timing.
- Calibrated levers (EventActionRangedAxis) for the Thrustmaster TCA quadrant, with a lever editor.
- PMDG and SimConnect event pickers, multi-aircraft binding, redesigned Variables view.
- Notices for unmapped inputs.

### Changed
- Profile merges and snippet imports are additive and no longer wipe device layouts; snippets are cached outside ./profiles.
- Built-in profiles are no longer written to ./profiles on startup.

### Fixed
- SimConnect wire protocol framing, port detection, client data definitions and SIMCONNECT_RECV_ID values (bridge data was never read).
- Layout clipping and missing glyphs in the UI.

## [0.1.0] - 2026-11-08

### Added
- Native flight controller hardware engine & controller bridge with Linux evdev and HID support.
- Fully local, user-owned Profile model with per-device layouts and single-step undo.
- Aircraft auto-switching matching sim-reported titles with support for wildcards and generic fallback profiles.
- One-time SPAD.neXt snippet importer targeting selected controller layouts with overwrite confirmation.
- Modern high-contrast dark and light themes meeting WCAG AA accessibility standards (>= 4.5:1 contrast).
- Virtualized lists for controllers, hardware inputs, and aircraft profiles using egui `show_rows`.
- UI scaling options (90%, 100%, 115%, 130%) with persistent zoom adjustments.
- Dedicated Profiles screen with left-side profile navigation and rich right-side layout manager.
- Two-column controller mapping view with live hardware input detection and visual activity indicators.
- Live axis calibration, deadzone configuration, and visual response curves.
- SimConnect network bridge and Steam Proton MSFS integration support.
- About dialog with license information, repository links, and non-affiliation disclaimers.
- Original vector and multi-size raster application icon (`assets/icon.svg`, `assets/icons/`, `assets/icon.ico`) wired into eframe window viewport and Linux desktop integration (`packaging/aerodeck.desktop`, hicolor icons).
- Fixed profile picker height and two-column scroll area sizing in Profiles and Controller Mapping screens.
- Fixed profile title truncation in details panel header by separating heading and action button bars into a responsive two-row card.
