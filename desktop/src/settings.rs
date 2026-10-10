//! What the reader decides about the program itself: starting with Windows,
//! where a closed window goes, and taking it all away again.
//!
//! THE SHELL KEEPS THESE, NOT THE PAGE. Whether to start hidden is decided
//! before any page exists, and a setting the window cannot read until it has
//! rendered is a setting that flashes the window it was meant to hide.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Off until asked for: it is the one thing this program leaves in the
    /// registry, so it is the reader's to choose.
    pub autostart: bool,
    /// Started by Windows, open straight into the tray.
    pub start_minimized: bool,
    /// The close button hides the window and the tray icon keeps the app —
    /// and any computing together — alive. Quit is in the tray menu.
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { autostart: false, start_minimized: false, close_to_tray: true }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// One switch by the name the page uses; an unknown name is refused rather
    /// than ignored, so a page newer than this shell learns it.
    pub fn set(&mut self, key: &str, on: bool) -> Result<(), String> {
        match key {
            "autostart" => self.autostart = on,
            "start_minimized" => self.start_minimized = on,
            "close_to_tray" => self.close_to_tray = on,
            _ => return Err(format!("no setting {key}")),
        }
        Ok(())
    }
}

/// The argument Windows starts the program with, so a launch at sign-in can
/// tell itself from one the reader asked for.
pub const AUTOSTART_ARG: &str = "--autostart";

#[cfg(windows)]
mod run_key {
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "WFSim";
    /// ERROR_FILE_NOT_FOUND: deleting a value that is not there is done.
    const NOT_FOUND: u32 = 2;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn set(command: &str) -> Result<(), String> {
        let data = wide(command);
        // SAFETY: every pointer is a NUL-terminated UTF-16 buffer alive for the
        // call, and the size is the data's own length in bytes.
        let rc = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                wide(RUN).as_ptr(),
                wide(NAME).as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if rc == 0 { Ok(()) } else { Err(format!("could not add to startup (error {rc})")) }
    }

    pub fn clear() -> Result<(), String> {
        // SAFETY: as above.
        let rc = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), wide(NAME).as_ptr()) };
        if rc == 0 || rc == NOT_FOUND { Ok(()) } else { Err(format!("could not remove from startup (error {rc})")) }
    }
}

/// Make the startup entry say what `on` says. WRITTEN AGAIN ON EVERY LAUNCH
/// while on: the program is a file the reader can move, and an entry naming
/// its old location starts nothing.
pub fn apply_autostart(on: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            run_key::set(&format!("\"{}\" {AUTOSTART_ARG}", exe.display()))
        } else {
            run_key::clear()
        }
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        Err("starting with the system is Windows only".into())
    }
}

/// EVERYTHING THIS PROGRAM PUT ON THE MACHINE, and the only list of it: the
/// startup entry, the app's directory (release, board, session, settings), the
/// webview's profile (every build, scenario and riven saved in the page) and
/// the executable itself.
///
/// The directories and the executable are in use until the process exits, so
/// they are removed by a command that waits for it. Each path is checked
/// against the name it must have: this deletes directories recursively, and a
/// wrong environment variable must not make that someone else's directory.
pub fn uninstall(data_root: &Path, webview_root: Option<PathBuf>) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;

        run_key::clear()?;
        let named = |p: &Path, want: &str| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(want));
        if !named(data_root, "WFSim") {
            return Err(format!("refusing to remove {}", data_root.display()));
        }
        let mut steps = vec![format!("rmdir /s /q \"{}\"", data_root.display())];
        if let Some(w) = webview_root.filter(|w| named(w, "app.wfsim.desktop")) {
            steps.push(format!("rmdir /s /q \"{}\"", w.display()));
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        steps.push(format!("del /f /q \"{}\"", exe.display()));
        // `ping` is the wait: `timeout` needs a console, and this has none.
        let script = format!("ping -n 3 127.0.0.1 >nul & {}", steps.join(" & "));
        std::process::Command::new("cmd")
            .raw_arg(format!("/d /s /c \"{script}\""))
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (data_root, webview_root);
        Err("uninstall is Windows only".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A FILE FROM AN OLDER SHELL, missing a setting, reads that setting's
    /// default rather than resetting the rest.
    #[test]
    fn a_missing_setting_takes_its_default_and_keeps_the_others() {
        let s: Settings = serde_json::from_str(r#"{"autostart":true}"#).unwrap();
        assert_eq!(s, Settings { autostart: true, ..Settings::default() });
        assert!(Settings::default().close_to_tray);
        assert!(!Settings::default().autostart);
    }

    #[test]
    fn an_unknown_setting_is_refused() {
        let mut s = Settings::default();
        assert!(s.set("autostart", true).is_ok() && s.autostart);
        assert!(s.set("launch_rockets", true).is_err());
    }
}
