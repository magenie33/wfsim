//! Where the app lives on disk, and the three states that make an update
//! reversible.
//!
//!   current/   what the webview serves. The only directory the app reads.
//!   next/      what the updater is assembling. Incomplete by definition.
//!   prev/      the version `current` replaced. The way back.
//!
//! A SWAP IS TWO RENAMES, and renames within one volume are atomic, so there
//! is no moment where `current/` is half-written. There IS a moment where it
//! does not exist — between the two renames — and `open()` handles it: a
//! missing `current` is recovered from `prev`, and failing that from the
//! embedded payload. That is the difference between a crash during an update
//! costing a restart and costing a reinstall.
//!
//! BOOT HEALTH is the other half. The shell cannot know whether a new version
//! works, so it counts: every launch increments a counter before showing
//! anything, and the page clears it once it has actually rendered. Two launches
//! that never got that far mean the current version cannot start, and the shell
//! goes back to `prev` on its own. This is the ONE piece of recovery that
//! cannot live in JavaScript — it has to run before any of it does.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::payload;

/// Two, not one: a single failure is as likely to be a machine going to sleep
/// mid-launch as a broken build, and rolling back on that would undo a good
/// update for a reason that has nothing to do with it.
const MAX_FAILED_BOOTS: u32 = 2;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Boot {
    fails: u32,
}

pub struct Layout {
    root: PathBuf,
}

/// Where this app keeps the copy of the site it serves.
///
/// EACH PLATFORM'S OWN CONVENTION, rather than one path with the others bolted
/// on. It is the directory a reader deletes to remove the app — the executable
/// is the only other thing there is — so it has to be where they would look.
///
/// The name differs with the convention too: `WFSim` where directories are
/// capitalised, `wfsim` where they are not.
pub fn data_root() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("WFSim")
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("WFSim")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // XDG: `$XDG_DATA_HOME`, or its documented default.
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("wfsim")
    }
}

impl Layout {
    pub fn open() -> std::io::Result<Self> {
        let me = Self { root: data_root() };
        std::fs::create_dir_all(&me.root)?;

        // A `next/` left behind is an update that died partway. It is never
        // salvageable — the manifest check that would have promoted it never
        // ran — so it is discarded rather than resumed.
        let _ = std::fs::remove_dir_all(me.next());

        if !me.current().join("index.html").exists() {
            if me.prev().join("index.html").exists() {
                std::fs::rename(me.prev(), me.current())?;
            } else {
                let _ = std::fs::remove_dir_all(me.current());
                std::fs::create_dir_all(me.current())?;
                payload::unpack_to(&me.current())?;
                me.write_manifest(&payload::manifest())?;
            }
        }
        Ok(me)
    }

    pub fn current(&self) -> PathBuf { self.root.join("current") }
    pub fn next(&self) -> PathBuf { self.root.join("next") }
    fn prev(&self) -> PathBuf { self.root.join("prev") }
    fn boot_file(&self) -> PathBuf { self.root.join("boot.json") }
    fn manifest_file(&self) -> PathBuf { self.current().join(".manifest.json") }

    pub fn manifest(&self) -> payload::Manifest {
        std::fs::read(self.manifest_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            // A missing manifest means a directory this shell did not write.
            // The embedded one is the honest answer: it describes what a fresh
            // install holds, and the updater will replace whatever differs.
            .unwrap_or_else(payload::manifest)
    }

    pub fn write_manifest(&self, m: &payload::Manifest) -> std::io::Result<()> {
        std::fs::write(self.manifest_file(), serde_json::to_vec_pretty(m)?)
    }

    /// Called before the window is shown. Returns true if this launch rolled
    /// back — the caller says so on screen, because a silent downgrade is
    /// indistinguishable from an update that never happened.
    pub fn boot_begin(&self) -> bool {
        let mut boot: Boot = std::fs::read(self.boot_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();

        let rolled = if boot.fails >= MAX_FAILED_BOOTS && self.prev().join("index.html").exists() {
            let dead = self.root.join("broken");
            let _ = std::fs::remove_dir_all(&dead);
            let _ = std::fs::rename(self.current(), &dead);
            let ok = std::fs::rename(self.prev(), self.current()).is_ok();
            if ok {
                let _ = std::fs::remove_dir_all(&dead);
            }
            boot.fails = 0;
            ok
        } else {
            false
        };

        boot.fails += 1;
        let _ = std::fs::write(self.boot_file(), serde_json::to_vec(&boot).unwrap_or_default());
        rolled
    }

    /// Called by the page once it has rendered. Only this clears the counter.
    pub fn boot_healthy(&self) {
        let _ = std::fs::write(self.boot_file(), br#"{"fails":0}"#);
    }

    /// Promote `next/` to `current/`, keeping the old one as `prev/`.
    pub fn promote(&self) -> std::io::Result<()> {
        if !self.next().join("index.html").exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "next/ has no index.html — refusing to promote an incomplete update",
            ));
        }
        let _ = std::fs::remove_dir_all(self.prev());
        std::fs::rename(self.current(), self.prev())?;
        match std::fs::rename(self.next(), self.current()) {
            Ok(()) => Ok(()),
            // The window between the two renames is the only moment `current`
            // does not exist. Putting `prev` back closes it here rather than
            // leaving it to the next launch.
            Err(e) => {
                let _ = std::fs::rename(self.prev(), self.current());
                Err(e)
            }
        }
    }
}

/// Reject anything that could climb out of the served directory. A local app
/// is not a hostile environment, but the handler takes a path from a web page
/// and a page can be handed a URL by anyone (a share link, a pasted address).
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(char::from(92)) || part.contains(':') {
            return None;
        }
        out.push(part);
    }
    Some(out)
}
