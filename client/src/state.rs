// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const CONSENT_VERSION: u32 = 1;
pub const CONSENT: &str = "WFSim runs simulation and optimization tasks for every player on this device.\nResults count when independent volunteers agree. Participation is voluntary and unpaid;\nthe results are free for every player and WFSim earns no income from this computing.\nComputing uses electricity and produces heat. It pauses on battery and low memory.\nYour CPU range controls an average budget between compute pieces, not an instantaneous OS limit.\nUse wfsim pause, wfsim stop, or Ctrl+C to stop accepting work.\nDevice identity, task answers and measured work are sent to WFSim for verification and contribution credit.";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Range {
    pub min: u8,
    pub max: u8,
}

impl std::str::FromStr for Range {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        let (lo, hi) = s.split_once('-').ok_or("use MIN-MAX, for example 0-50")?;
        let min: u8 = lo
            .parse()
            .map_err(|_| "minimum must be an integer from 0 to 100")?;
        let max: u8 = hi
            .parse()
            .map_err(|_| "maximum must be an integer from 0 to 100")?;
        if min > max || max > 100 {
            return Err("require 0 <= minimum <= maximum <= 100".into());
        }
        Ok(Self { min, max })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Consent {
    pub v: u32,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub server: String,
    pub range: Range,
    pub name: String,
    pub paused: bool,
    pub consent: Option<Consent>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: "https://wfsim.app".into(),
            range: Range { min: 0, max: 50 },
            name: "WFSim CLI".into(),
            paused: false,
            consent: None,
        }
    }
}

impl Config {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("config.json");
        let mut c: Self = if path.exists() {
            read(&path)?
        } else {
            Self::default()
        };
        if c.range.min > c.range.max || c.range.max > 100 {
            return Err("invalid CPU range in config.json".into());
        }
        let u = url::Url::parse(&c.server)?;
        if u.scheme() != "https"
            && !(u.scheme() == "http"
                && matches!(
                    u.host_str(),
                    Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
                ))
        {
            return Err("server must use HTTPS (HTTP is allowed only on loopback)".into());
        }
        if u.path() != "/"
            || !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
        {
            return Err(
                "server must be an origin without credentials, path, query or fragment".into(),
            );
        }
        c.server = u.origin().ascii_serialization();
        Ok(c)
    }
    pub fn save(&self, dir: &Path) -> Result<()> {
        write(&dir.join("config.json"), self)
    }
}

pub fn data_dir() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("WFSIM_CLI_HOME") {
        return Ok(p.into());
    }
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("WFSim"));
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|p| p.join("Library/Application Support/WFSim"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
        .map(|p| p.join("wfsim"));
    Ok(base
        .ok_or("cannot find a user data directory; use --data-dir")?
        .join("cli"))
}

pub fn random_hex(n: usize) -> Result<String> {
    let mut b = vec![0; n];
    getrandom::getrandom(&mut b).map_err(|e| format!("random identity: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

pub fn identity(dir: &Path) -> Result<String> {
    let path = dir.join("identity");
    if path.exists() {
        let id = fs::read_to_string(path)?;
        if id.len() != 24
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("device identity is damaged; restore it from your backup".into());
        }
        return Ok(id);
    }
    let id = random_hex(12)?;
    let mut f = private_options().create_new(true).open(&path)?;
    f.write_all(id.as_bytes())?;
    f.sync_all()?;
    if fs::read_to_string(path)? != id {
        return Err("cannot persist device identity".into());
    }
    Ok(id)
}

pub fn directory(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn private_options() -> OpenOptions {
    let mut o = OpenOptions::new();
    o.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o
}

pub fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", random_hex(8)?));
    let r = (|| -> Result<()> {
        let mut f = private_options().create_new(true).open(&temporary)?;
        serde_json::to_writer(&mut f, value)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        // Windows cannot rename over an existing file; ReplaceFileW is atomic.
        #[cfg(windows)]
        replace(&temporary, path)?;
        #[cfg(not(windows))]
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if r.is_err() {
        let _ = fs::remove_file(temporary);
    }
    r
}

#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both strings are terminated UTF-16; replace existing + write through.
    if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamp")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pending {
    pub engine: String,
    pub path: String,
    pub body: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integer_ranges_include_zero_and_fixed_full_power() {
        for s in ["0-0", "0-50", "1-99", "100-100"] {
            assert!(s.parse::<Range>().is_ok());
        }
        for s in ["50-0", "0-101", "0.1-50", "-1-50", "100"] {
            assert!(s.parse::<Range>().is_err());
        }
    }
    #[test]
    fn identity_survives_and_damage_is_never_silently_reset() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("wfsim-client-{}", random_hex(8)?));
        directory(&dir)?;
        let id = identity(&dir)?;
        assert_eq!(id, identity(&dir)?);
        write(
            &dir.join("pending.json"),
            &vec![Pending {
                engine: "engine".into(),
                path: "/sync".into(),
                body: serde_json::json!({"score":1.2345678901234567}),
            }],
        )?;
        assert_eq!(
            read::<Vec<Pending>>(&dir.join("pending.json"))?[0].body["score"],
            serde_json::json!(1.2345678901234567)
        );
        fs::write(dir.join("identity"), "broken")?;
        assert!(identity(&dir).is_err());
        fs::remove_dir_all(dir)?;
        Ok(())
    }
}
