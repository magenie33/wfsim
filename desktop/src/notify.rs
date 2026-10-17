//! A NOTIFICATION FROM THE PAGE, shown the way Windows shows its own —
//! docs/DESKTOP.md §"Notifications". The page decides what to say and when
//! (`noticeSystem`, app.js); the shell only shows it and, clicked, brings the
//! window back on the page it names.

use std::path::Path;

/// The name Windows files the notifications under, and the identity the
/// webview's own data already carries (`tauri.conf.json` `identifier`).
pub const AUMID: &str = "app.wfsim.desktop";

/// A program with no installer has no shortcut for Windows to learn its name
/// and icon from, so it states them under `AppUserModelId` in the reader's own
/// hive. Without it each notification says it came from PowerShell.
#[cfg(windows)]
mod registration {
    use windows_sys::Win32::System::Registry::{RegDeleteTreeW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

    const KEY: &str = r"Software\Classes\AppUserModelId\app.wfsim.desktop";
    /// ERROR_FILE_NOT_FOUND: removing what is not there is done.
    const NOT_FOUND: u32 = 2;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn set(name: &str, value: &str) -> Result<(), String> {
        let data = wide(value);
        // SAFETY: every pointer is a NUL-terminated UTF-16 buffer alive for the
        // call, and the size is the data's own length in bytes.
        let rc = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide(name).as_ptr(), REG_SZ,
                data.as_ptr().cast(), (data.len() * 2) as u32)
        };
        if rc == 0 { Ok(()) } else { Err(format!("could not register for notifications (error {rc})")) }
    }

    pub fn write(icon: &str) -> Result<(), String> {
        set("DisplayName", "WFSim")?;
        set("IconUri", icon)
    }

    pub fn clear() -> Result<(), String> {
        // SAFETY: as above.
        let rc = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(KEY).as_ptr()) };
        if rc == 0 || rc == NOT_FOUND { Ok(()) } else { Err(format!("could not remove the notification entry (error {rc})")) }
    }
}

/// WRITTEN ONCE PER RUN, before the first notification: the program is a file
/// the reader can move, and the icon lives beside the app's data.
static REGISTERED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();

#[cfg(windows)]
fn register(root: &Path) -> Result<(), String> {
    REGISTERED.get_or_init(|| {
        let icon = root.join("notify-icon.png");
        std::fs::write(&icon, include_bytes!("../icons/128x128.png")).map_err(|e| e.to_string())?;
        registration::write(&icon.display().to_string())
    }).clone()
}

/// The entry `register` wrote, for `settings::uninstall`.
pub fn clear() -> Result<(), String> {
    #[cfg(windows)]
    {
        registration::clear()
    }
    #[cfg(not(windows))]
    Ok(())
}

/// SHOW ONE. `on_open` runs when the reader clicks it.
pub fn show(root: &Path, title: &str, body: &str, on_open: impl Fn() + Send + 'static) -> Result<(), String> {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::{Duration, Toast};
        register(root)?;
        Toast::new(AUMID)
            .title(title)
            .text1(body)
            .duration(Duration::Short)
            .on_activated(move |_| {
                on_open();
                Ok(())
            })
            .show()
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = (root, title, body, on_open);
        Err("notifications are Windows only".into())
    }
}
