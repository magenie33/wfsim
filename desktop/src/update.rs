//! Replacing `current/` with a newer copy, without an installer.
//!
//! THE SHAPE. A signed manifest lists every file with its SHA-256. The updater
//! compares it with the one describing `current/`, downloads only what differs,
//! takes everything else from `current/` itself, and hands the finished
//! directory to `Layout::promote`. So a release of any size costs the bytes
//! that actually changed — an engine change is the wasm module and `app.js`,
//! about 1.5 MB out of 29.
//!
//! WHY IT IS NOT TAURI'S UPDATER. That one replaces the executable, which means
//! an installer, a UAC prompt on some machines, and an antivirus product
//! watching a program rewrite itself. This project releases often enough that
//! such an update would be a weekly event, so the FREQUENT path has to be the
//! quiet one: files in a directory, swapped by two renames. The shell itself
//! changes rarely and can keep the noisy path.
//!
//! SIGNATURES ARE NOT OPTIONAL. The bucket is public-read, over a network this
//! app exists because it is unreliable. Without a signature the update channel
//! is a way to run arbitrary code on every reader's machine; with one, the
//! worst a hostile network achieves is refusing to let updates through.
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::payload::Manifest;

/// From `updatekit keygen`. The private half lives in `private/` and in a
/// backup, never in this repository.
///
/// A LIST, NEVER ONE KEY, and the reason is that this array is compiled in. A
/// shell that accepts exactly one key cannot be handed a signature from a new
/// one, so losing or rotating the private half would leave every reader with a
/// dead update path and a manual reinstall as the only way back — which is the
/// outcome this client exists to remove. Rotation is: ship a shell accepting
/// both, wait for it to spread, then publish under the new key and drop the old
/// from this list.
const PUBLIC_KEYS: &[&str] = &[
    "b488481177cb0a459a685c5d66f646768e7f66c405b92ecde81025581419a95e",
];

/// Tried in order until one answers. MEASURED 2026-08-26 from Shanghai:
/// COS 9.73 MB/s, wfsim.app (Cloudflare) 2.11 MB/s — so the bucket leads and
/// the site is the fallback for the day the bucket is unreachable.
///
/// A MANIFEST MAY REPLACE THIS LIST (its `sources`), which is what makes the
/// channel survive its own hosting: moving to another provider, or adding one,
/// costs a manifest rather than a new installer. Hard-coding one origin would
/// make that origin's bad day every reader's reinstall.
const DEFAULT_SOURCES: &[&str] = &[
    "https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com",
    "https://wfsim.app",
];

const NET_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    /// idle | checking | uptodate | available | downloading | ready | failed
    pub phase: String,
    pub version: String,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub message: String,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            version: String::new(),
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
            message: String::new(),
        }
    }
}

static STATUS: Mutex<Option<Status>> = Mutex::new(None);

pub fn status() -> Status {
    STATUS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

fn set(s: Status) {
    if let Ok(mut g) = STATUS.lock() {
        *g = Some(s);
    }
}

/// Enter a phase synchronously, before the worker thread that will do the work.
///
/// THE COMMAND RETURNS BEFORE THE WORK STARTS, and the page polls. So if the
/// status still reads `available` when `update_download` returns, the page sees
/// nothing running, stops polling, and the download proceeds invisibly: the
/// button appears to do nothing and the update turns up on some later launch.
/// Setting the phase here closes that window — by the time the command returns,
/// the status already says what is about to happen.
pub fn begin(phase: &str) {
    let mut s = status();
    s.phase = phase.to_string();
    s.message = String::new();
    set(s);
}

pub fn note_failure(message: &str) {
    let mut s = status();
    s.phase = "failed".into();
    s.message = message.to_string();
    set(s);
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .timeout(NET_TIMEOUT)
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{url}: {e}"))?;
    Ok(buf)
}

/// Verify a signature over RAW BYTES.
///
/// Signing the bytes rather than a parsed structure keeps this independent of
/// how either side serializes JSON: two encoders disagree about key order and
/// spacing, and a signature that depends on which one ran fails at random.
///
/// ANY ACCEPTED KEY IS ENOUGH — see `PUBLIC_KEYS` for why there is more than
/// one. A malformed entry is skipped rather than fatal: it would otherwise take
/// the whole update path down at the moment a rotation is being prepared.
fn verify(body: &[u8], sig_hex: &str) -> Result<(), String> {
    let sig: [u8; 64] = hex_to_bytes(sig_hex)
        .and_then(|v| v.try_into().ok())
        .ok_or("signature is malformed")?;
    let sig = Signature::from_bytes(&sig);
    let ok = PUBLIC_KEYS.iter().any(|k| {
        hex_to_bytes(k)
            .and_then(|v| <[u8; 32]>::try_from(v).ok())
            .and_then(|k| VerifyingKey::from_bytes(&k).ok())
            .is_some_and(|vk| vk.verify(body, &sig).is_ok())
    });
    if ok {
        Ok(())
    } else {
        Err("SIGNATURE DOES NOT MATCH - refusing this update".to_string())
    }
}

fn sources(local: &Manifest) -> Vec<String> {
    if local.sources.is_empty() {
        DEFAULT_SOURCES.iter().map(|s| (*s).to_string()).collect()
    } else {
        local.sources.clone()
    }
}

/// Where to fetch the source archive for what this client is RUNNING.
///
/// AGPL asks for the source corresponding to the binary being conveyed, and
/// this client conveys itself a new one every time it updates — so the answer
/// moves with `current/`, and a link to "the latest source" would be the wrong
/// tree for anyone who has not updated yet. `release_desktop.py` keeps every
/// version's archive at `src/<version>/source.zip` for exactly this.
pub fn source_url(local: &Manifest) -> String {
    let base = sources(local)
        .first()
        .map(|b| b.trim_end_matches('/').to_string())
        .unwrap_or_default();
    format!("{base}/src/{}/source.zip", local.version)
}

/// A signed document that carries its own signature — see `pointer`.
#[derive(serde::Deserialize)]
struct SignedDoc {
    sig: String,
    signed: String,
}

/// What the pointer says. `sources` replaces the local list, which is what lets
/// the channel be moved without shipping a new shell.
#[derive(serde::Deserialize)]
struct Pointer {
    manifest: String,
    #[serde(default)]
    sources: Vec<String>,
}

/// THE CURRENT PATH: one signed pointer, then the manifest it names.
///
/// The signature travels INSIDE the pointer because two objects fetched
/// separately can be torn: a client reading between the two puts of a publish
/// got a new manifest beside an old detached signature and was told its update
/// failed verification — which reads as an attack and is a publish landing
/// mid-fetch.
///
/// The manifest is then addressed by the digest that was just verified, so what
/// comes back is checked against what was signed rather than trusted for having
/// arrived from the same host.
fn pointer_manifest(base: &str) -> Result<Manifest, String> {
    let doc: SignedDoc = serde_json::from_slice(&fetch(&format!("{base}/channel.json"))?)
        .map_err(|e| format!("channel.json is not valid: {e}"))?;
    verify(doc.signed.as_bytes(), &doc.sig)?;
    let p: Pointer = serde_json::from_str(&doc.signed)
        .map_err(|e| format!("the pointer is not valid: {e}"))?;

    let body = fetch(&format!("{base}/manifest/{}.json", p.manifest))?;
    let got = format!("{:x}", Sha256::digest(&body));
    if got != p.manifest {
        return Err(format!(
            "manifest {got} is not the {} the pointer named - refusing this update",
            p.manifest
        ));
    }
    let mut m: Manifest =
        serde_json::from_slice(&body).map_err(|e| format!("manifest is not valid: {e}"))?;
    if !p.sources.is_empty() {
        m.sources = p.sources;
    }
    Ok(m)
}

/// THE PATH SHELLS OLDER THAN THE POINTER READ, kept because a shell only
/// changes with an installer: taking it away would leave those readers with no
/// update path and no way to be told so.
fn legacy_manifest(base: &str) -> Result<Manifest, String> {
    let body = fetch(&format!("{base}/manifest.json"))?;
    let sig = fetch(&format!("{base}/manifest.json.sig"))?;
    verify(&body, &String::from_utf8_lossy(&sig))?;
    serde_json::from_slice(&body).map_err(|e| format!("manifest is not valid: {e}"))
}

/// Fetch and verify the remote manifest, from the first source that answers.
fn remote_manifest(local: &Manifest) -> Result<Manifest, String> {
    let mut last = String::from("no sources configured");
    for base in sources(local) {
        let base = base.trim_end_matches('/');
        match pointer_manifest(base).or_else(|e| {
            last = e;
            legacy_manifest(base)
        }) {
            Ok(m) => return Ok(m),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn missing<'a>(local: &Manifest, remote: &'a Manifest) -> Vec<&'a crate::payload::Entry> {
    let have: std::collections::HashMap<&str, &str> =
        local.files.iter().map(|e| (e.p.as_str(), e.h.as_str())).collect();
    remote
        .files
        .iter()
        .filter(|e| have.get(e.p.as_str()) != Some(&e.h.as_str()))
        .collect()
}

pub fn check(local: &Manifest) -> Result<Status, String> {
    set(Status { phase: "checking".into(), ..Default::default() });

    let remote = remote_manifest(local)?;
    let needed = missing(local, &remote);

    let s = Status {
        // A REBUILD WITH NO CONTENT CHANGE IS NOT AN UPDATE. `site/` is
        // regenerated on every push, so versions differ constantly while the
        // files a reader would receive are byte-identical; announcing that as
        // an update would train people to ignore the notice.
        phase: if needed.is_empty() { "uptodate".into() } else { "available".into() },
        version: remote.version.clone(),
        files_total: needed.len(),
        bytes_total: needed.iter().map(|e| e.n as u64).sum(),
        ..Default::default()
    };
    set(s.clone());
    Ok(s)
}

/// Assemble `next/` and leave it ready for `Layout::promote`.
///
/// Blocking: the caller runs it on its own thread and the page polls `status`.
pub fn download(local: &Manifest, current: &Path, next: &Path) -> Result<Manifest, String> {
    let remote = remote_manifest(local)?;
    let have: std::collections::HashMap<&str, &str> =
        local.files.iter().map(|e| (e.p.as_str(), e.h.as_str())).collect();

    let _ = std::fs::remove_dir_all(next);
    std::fs::create_dir_all(next).map_err(|e| e.to_string())?;

    let needed = missing(local, &remote);
    let mut s = Status {
        phase: "downloading".into(),
        version: remote.version.clone(),
        files_total: needed.len(),
        bytes_total: needed.iter().map(|e| e.n as u64).sum(),
        ..Default::default()
    };
    set(s.clone());

    let bases: Vec<String> = sources(local)
        .iter()
        .map(|b| b.trim_end_matches('/').to_string())
        .collect();

    for entry in &remote.files {
        let dest = crate::layout::safe_join(next, &entry.p)
            .ok_or_else(|| format!("manifest names an unsafe path: {}", entry.p))?;
        std::fs::create_dir_all(dest.parent().ok_or("no parent")?).map_err(|e| e.to_string())?;

        // UNCHANGED FILES ARE COPIED, NOT FETCHED. This is what makes an update
        // cost its diff: 764 files ship, a typical release changes three.
        if have.get(entry.p.as_str()) == Some(&entry.h.as_str())
            && std::fs::copy(current.join(&entry.p), &dest).is_ok()
        {
            continue;
        }

        // CONTENT-ADDRESSED, not `v/<version>/<path>`. Storing by version means
        // publishing 29 MB for a release that changed one file, and it means a
        // reader who skipped four versions re-downloads files that never
        // changed across them. Under `blob/<sha256>` the bucket holds each
        // distinct file once for ever, a publish uploads only what is new, and
        // an update fetches only what this reader is missing — regardless of
        // how many versions ago they last looked.
        //
        // …AND A SOURCE THAT KEEPS ITS FILES BY PATH ANSWERS TOO. wfsim.app is
        // the site, not a blob store: it has `app.js`, never `blob/<sha>`. The
        // bytes are checked against the manifest either way, so a path that has
        // moved on fails its hash and falls through — which makes the fallback
        // real rather than a second entry in a list that could never answer.
        let mut got = None;
        let mut last = String::new();
        for base in &bases {
            for url in [format!("{base}/blob/{}", entry.h), format!("{base}/{}", entry.p)] {
                match fetch(&url) {
                    Ok(b) if format!("{:x}", Sha256::digest(&b)) == entry.h => {
                        got = Some(b);
                        break;
                    }
                    Ok(_) => last = format!("{url}: not the file the manifest names"),
                    Err(e) => last = e,
                }
            }
            if got.is_some() {
                break;
            }
        }
        // NOTHING REACHES DISK THAT DID NOT HASH TO WHAT THE MANIFEST NAMED —
        // the loop above accepts a body only on that condition, so exhausting
        // every source means no source held this file, not that a wrong one was
        // taken.
        let bytes = got.ok_or_else(|| {
            format!("{} - refusing this update ({last})", entry.p)
        })?;
        std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", entry.p))?;

        s.files_done += 1;
        s.bytes_done += bytes.len() as u64;
        set(s.clone());
    }

    // WRITTEN LAST, because `next/.manifest.json` existing is what says the
    // directory is complete — and `promote` refuses one that is not.
    std::fs::write(
        next.join(".manifest.json"),
        serde_json::to_vec_pretty(&remote).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    s.phase = "ready".into();
    s.message = remote.version.clone();
    set(s.clone());
    Ok(remote)
}
