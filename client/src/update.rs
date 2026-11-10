// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    engine,
    net::Net,
    state::{self, Result},
    trust,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const HOST_CHANGED: i32 = 76;
pub const INTERVAL: Duration = Duration::from_secs(3600);
const LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub struct HostChanged;
impl std::fmt::Display for HostChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CLI update ready")
    }
}
impl std::error::Error for HostChanged {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signed {
    pub body: String,
    pub sig: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Installed {
    pub current: Option<Signed>,
    pub previous: Option<Signed>,
    pub highest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Host {
    pub signed: Signed,
    pub version: Version,
    pub file: Value,
    pub path: PathBuf,
}

fn target() -> String {
    format!(
        "{}-{}",
        match std::env::consts::ARCH {
            "arm64" => "aarch64",
            other => other,
        },
        match std::env::consts::OS {
            "macos" => "macos",
            other => other,
        }
    )
}

fn version(text: &str) -> Result<Version> {
    let parsed = Version::parse(text)?;
    if parsed.to_string() != text || !parsed.build.is_empty() || text.len() > 80 {
        return Err("invalid CLI host version".into());
    }
    Ok(parsed)
}

impl Host {
    pub fn parse(dir: &Path, signed: Signed, keys: &[&str]) -> Result<Self> {
        engine::verify_with_keys(signed.body.as_bytes(), &signed.sig, keys)?;
        let manifest: Value = serde_json::from_str(&signed.body)?;
        if manifest["v"] != 1 || manifest["launch"] != 1 {
            return Err("unsupported signed CLI launch contract".into());
        }
        let version = version(manifest["version"].as_str().ok_or("missing CLI version")?)?;
        let file = manifest["files"][target()].clone();
        let expected_name = format!(
            "wfsim-{}{}",
            target(),
            if cfg!(windows) { ".exe" } else { "" }
        );
        let sha = file["sha256"]
            .as_str()
            .ok_or("missing CLI platform build")?;
        if file["name"] != expected_name
            || sha.len() != 64
            || !sha.bytes().all(|b| b.is_ascii_hexdigit())
            || !file["size"].as_u64().is_some_and(|s| s > 0 && s <= LIMIT)
        {
            return Err("invalid signed CLI file descriptor".into());
        }
        let path = dir
            .join("hosts")
            .join("releases")
            .join(version.to_string())
            .join(if cfg!(windows) { "wfsim.exe" } else { "wfsim" });
        Ok(Self {
            signed,
            version,
            file,
            path,
        })
    }
    pub fn valid(&self) -> bool {
        fs::read(&self.path).is_ok_and(|bytes| self.matches(&bytes))
    }
    fn matches(&self, bytes: &[u8]) -> bool {
        bytes.len() as u64 == self.file["size"].as_u64().unwrap_or(0)
            && format!("{:x}", Sha256::digest(bytes)) == self.file["sha256"].as_str().unwrap_or("")
    }
}

pub fn installed(dir: &Path) -> Result<Installed> {
    let path = dir.join("hosts/state.json");
    if path.exists() {
        state::read(&path)
    } else {
        Ok(Installed::default())
    }
}

pub fn save(dir: &Path, state: &Installed) -> Result<()> {
    state::directory(&dir.join("hosts"))?;
    state::write(&dir.join("hosts/state.json"), state)
}

pub fn current(dir: &Path, state: &Installed, keys: &[&str]) -> Option<Host> {
    state
        .current
        .clone()
        .and_then(|s| Host::parse(dir, s, keys).ok())
        .filter(Host::valid)
}

pub fn check(
    dir: &Path,
    base: &Version,
    sources: &[String],
    keys: &[&str],
) -> Result<Option<Host>> {
    let installed = installed(dir)?;
    let highest = installed
        .highest
        .as_deref()
        .map(version)
        .transpose()?
        .unwrap_or_else(|| base.clone())
        .max(base.clone());
    let net = Net::new("https://wfsim.app");
    let mut errors = Vec::new();
    for source in sources {
        match (|| -> Result<Option<Host>> {
            let hint = net.get(&format!("{source}/cli/latest.json"))?.0;
            let next = version(hint["version"].as_str().ok_or("missing CLI version")?)?;
            if next <= highest || (base.pre.is_empty() && !next.pre.is_empty()) {
                return Ok(None);
            }
            let prefix = format!("{source}/cli/releases/{next}/release.json");
            let body = String::from_utf8(net.bytes(&prefix, 64 * 1024)?)?;
            let sig = String::from_utf8(net.bytes(&format!("{prefix}.sig"), 256)?)?;
            let host = Host::parse(
                dir,
                Signed {
                    body,
                    sig: sig.trim().into(),
                },
                keys,
            )?;
            if host.version != next {
                return Err("CLI pointer and signed release disagree".into());
            }
            if !host.valid() {
                let mut downloaded = None;
                let mut failure = "no usable download URL".to_string();
                for url in host.file["urls"]
                    .as_array()
                    .ok_or("missing CLI download URLs")?
                {
                    let Some(url) = url.as_str() else { continue };
                    let parsed = url::Url::parse(url)?;
                    if !parsed.username().is_empty()
                        || parsed.password().is_some()
                        || parsed.fragment().is_some()
                    {
                        continue;
                    }
                    let loopback = matches!(
                        parsed.host_str(),
                        Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
                    );
                    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
                        continue;
                    }
                    match net.bytes(url, host.file["size"].as_u64().unwrap_or(0)) {
                        Ok(bytes) if host.matches(&bytes) => {
                            downloaded = Some(bytes);
                            break;
                        }
                        Ok(_) => {
                            failure = "downloaded bytes do not match the signed size/hash".into()
                        }
                        Err(error) => failure = error.to_string(),
                    }
                }
                let bytes = downloaded.ok_or_else(|| {
                    format!("CLI download refused: {failure}; keeping the installed version")
                })?;
                let parent = host.path.parent().ok_or("missing CLI directory")?;
                state::directory(parent)?;
                let temporary = parent.join(format!("{}.download", state::random_hex(8)?));
                let result = (|| -> Result<()> {
                    let mut file = state::private_options().create_new(true).open(&temporary)?;
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    executable(&temporary)?;
                    if host.valid() {
                        fs::remove_file(&temporary)?;
                    } else {
                        #[cfg(windows)]
                        state::replace(&temporary, &host.path)?;
                        #[cfg(not(windows))]
                        fs::rename(&temporary, &host.path)?;
                    }
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temporary);
                }
                result?;
            }
            probe(&host.path, &host.version)?;
            Ok(Some(host))
        })() {
            Ok(host) => return Ok(host),
            Err(error) => errors.push(error.to_string()),
        }
    }
    Err(errors.join("; ").into())
}

fn probe(path: &Path, expected: &Version) -> Result<()> {
    let mut child = Command::new(path)
        .arg("--version")
        .env("WFSIM_CLI_NO_DELEGATE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err("new CLI version probe timed out".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output()?;
    if !output.status.success()
        || output.stdout != format!("wfsim {expected}\n").as_bytes()
            && output.stdout != format!("wfsim {expected}\r\n").as_bytes()
    {
        return Err("new CLI build does not report the signed version".into());
    }
    Ok(())
}

fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub fn launcher(dir: &Path) -> Result<PathBuf> {
    let root = dir.join("launcher");
    state::directory(&root)?;
    let path = root.join(if cfg!(windows) { "wfsim.exe" } else { "wfsim" });
    if !path.exists() {
        let source = std::env::var_os("WFSIM_CLI_LAUNCHER")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_exe()?);
        copy_executable(&source, &path)?;
    }
    Ok(path)
}

pub fn delegate() -> Result<Option<i32>> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if std::env::var_os("WFSIM_CLI_NO_DELEGATE").is_some()
        || std::env::var_os("WFSIM_CLI_RUN").is_some()
        || args.iter().any(|a| a == "join")
    {
        return Ok(None);
    }
    let mut dir: Option<PathBuf> = None;
    for (i, arg) in args.iter().enumerate() {
        if arg == "--data-dir" {
            if let Some(next) = args.get(i + 1) {
                dir = Some(next.into());
            }
        } else if let Some(value) = arg.to_str().and_then(|s| s.strip_prefix("--data-dir=")) {
            dir = Some(value.into());
        }
    }
    let dir = dir.map(Ok).unwrap_or_else(state::data_dir)?;
    let installed = match installed(&dir) {
        Ok(installed) => installed,
        Err(error) => {
            eprintln!("Cached CLI state unavailable: {error}; using the installed command.");
            return Ok(None);
        }
    };
    if let Some(host) = current(&dir, &installed, trust::PUBLIC_KEYS)
        .filter(|h| h.version > version(crate::BUILD).unwrap_or_else(|_| Version::new(0, 0, 0)))
    {
        let status = Command::new(host.path)
            .args(args)
            .env("WFSIM_CLI_NO_DELEGATE", "1")
            .env("WFSIM_CLI_LAUNCHER", std::env::current_exe()?)
            .status()?;
        return Ok(Some(status.code().unwrap_or(1)));
    }
    Ok(None)
}

pub fn bootstrap(dir: &Path) -> Result<PathBuf> {
    let root = dir
        .join("launcher")
        .join(version(crate::BUILD)?.to_string());
    state::directory(&root)?;
    let dest = root.join(if cfg!(windows) { "wfsim.exe" } else { "wfsim" });
    copy_executable(&std::env::current_exe()?, &dest)?;
    Ok(dest)
}

fn copy_executable(source: &Path, dest: &Path) -> Result<()> {
    let bytes = fs::read(source)?;
    if fs::read(dest).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let temporary = dest.with_extension(format!("{}.download", state::random_hex(8)?));
    let result = (|| -> Result<()> {
        let mut out = state::private_options().create_new(true).open(&temporary)?;
        out.write_all(&bytes)?;
        out.sync_all()?;
        executable(&temporary)?;
        #[cfg(windows)]
        state::replace(&temporary, dest)?;
        #[cfg(not(windows))]
        fs::rename(&temporary, dest)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn sources(server: &str) -> Vec<String> {
    if server == "https://wfsim.app" {
        trust::DEFAULT_SOURCES
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![server.to_string()]
    }
}

pub fn prune(dir: &Path, installed: &Installed) -> Result<()> {
    let keep = [&installed.current, &installed.previous]
        .into_iter()
        .flatten()
        .filter_map(|s| serde_json::from_str::<Value>(&s.body).ok())
        .filter_map(|m| m["version"].as_str().map(String::from))
        .collect::<Vec<_>>();
    let root = dir.join("hosts/releases");
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() && version(&name).is_ok() && !keep.contains(&name) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
