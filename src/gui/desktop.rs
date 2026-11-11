//! Desktop integration for the window icon on Wayland.
//!
//! Wayland clients cannot hand the compositor a window icon (winit has no
//! `xdg-toplevel-icon` support): the compositor looks up the `.desktop` file whose name
//! matches the app id (`aerodeck`) and uses its `Icon`. When AeroDeck runs without being
//! installed (e.g. `cargo run`), this writes a hidden `aerodeck.desktop` and the hicolor
//! icons into `~/.local/share`, unless a real installation already provides them.

use std::fs;
use std::path::{Path, PathBuf};

const MARKER: &str = "X-AeroDeck-Managed=true";

const ICONS: [(&str, &[u8]); 8] = [
    ("16x16/apps/aerodeck.png", include_bytes!("../../packaging/icons/hicolor/16x16/apps/aerodeck.png")),
    ("32x32/apps/aerodeck.png", include_bytes!("../../packaging/icons/hicolor/32x32/apps/aerodeck.png")),
    ("48x48/apps/aerodeck.png", include_bytes!("../../packaging/icons/hicolor/48x48/apps/aerodeck.png")),
    ("64x64/apps/aerodeck.png", include_bytes!("../../packaging/icons/hicolor/64x64/apps/aerodeck.png")),
    ("128x128/apps/aerodeck.png", include_bytes!("../../packaging/icons/hicolor/128x128/apps/aerodeck.png")),
    ("256x256/apps/aerodeck.png", include_bytes!("../../packaging/icons/hicolor/256x256/apps/aerodeck.png")),
    ("512x512/apps/aerodeck.png", include_bytes!("../../packaging/icons/hicolor/512x512/apps/aerodeck.png")),
    ("scalable/apps/aerodeck.svg", include_bytes!("../../packaging/icons/hicolor/scalable/apps/aerodeck.svg")),
];

fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

fn data_dirs() -> Vec<PathBuf> {
    let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from).collect()
}

/// Writes `content` to `path` if it differs. Returns true when the file changed.
fn write_if_changed(path: &Path, content: &[u8]) -> std::io::Result<bool> {
    if fs::read(path).is_ok_and(|old| old == content) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, content)?;
    Ok(true)
}

fn desktop_entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=AeroDeck\n\
         Comment=Flight simulator hardware engine and controller bridge\n\
         Exec=\"{}\"\n\
         Icon=aerodeck\n\
         Terminal=false\n\
         Categories=Utility;Game;Simulation;\n\
         StartupWMClass=aerodeck\n\
         NoDisplay=true\n\
         {}\n",
        exec.display(),
        MARKER
    )
}

/// Makes sure the compositor can find AeroDeck's icon. Errors are logged, never fatal.
pub fn ensure_installed() {
    if let Err(e) = try_install() {
        tracing::debug!("Desktop integration skipped: {}", e);
    }
}

fn try_install() -> std::io::Result<()> {
    let Some(home) = data_home() else { return Ok(()) };
    let local = home.join("applications/aerodeck.desktop");

    // A real installation (system-wide, or a user's own entry) wins: leave it alone.
    let installed_elsewhere = data_dirs().iter().any(|d| d.join("applications/aerodeck.desktop").exists());
    let user_owned = fs::read_to_string(&local).is_ok_and(|s| !s.contains(MARKER));
    if installed_elsewhere || user_owned {
        return Ok(());
    }

    let mut changed = false;
    for (rel, bytes) in ICONS {
        changed |= write_if_changed(&home.join("icons/hicolor").join(rel), bytes)?;
    }
    let exe = std::env::current_exe()?;
    changed |= write_if_changed(&local, desktop_entry(&exe).as_bytes())?;

    if changed {
        tracing::info!("Installed desktop entry and icons in {}", home.display());
        // Let KDE pick up the new entry right away (other desktops watch the directory).
        std::thread::spawn(|| {
            for tool in ["kbuildsycoca6", "kbuildsycoca5"] {
                let ran = std::process::Command::new(tool)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok();
                if ran {
                    break;
                }
            }
        });
    }
    Ok(())
}
