# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
