// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    net::Net,
    state::{self, Result},
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::trust;

type Api = unsafe extern "C" fn(*const u8, usize, *const u8, usize, *mut usize) -> *mut u8;
type Install = unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8;
type Free = unsafe extern "C" fn(*mut u8, usize);

pub struct Engine {
    pub id: String,
    api: Api,
    free: Free,
    // Native statics and TLS remain mapped until this compute process exits.
    _library: std::mem::ManuallyDrop<libloading::Library>,
}

impl Engine {
    pub fn load(net: &Net, dir: &Path, id: &str) -> Result<Self> {
        if id.is_empty()
            || id.len() > 40
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err("invalid engine identity".into());
        }
        let cache = dir.join("engines");
        state::directory(&cache)?;
        let target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
        let manifest_path = cache.join(format!("manifest.{id}.json"));
        let cached = state::read::<Value>(&manifest_path).ok().filter(|pointer| {
            let Some(signed) = pointer["signed"].as_str() else {
                return false;
            };
            let Some(sig) = pointer["sig"].as_str() else {
                return false;
            };
            verify(signed.as_bytes(), sig).is_ok()
                && serde_json::from_str::<Value>(signed).is_ok_and(|body| {
                    body["engine"] == id && body["abi"] == 2 && body["files"][&target].is_object()
                })
        });
        let mut errors = Vec::new();
        for base in trust::DEFAULT_SOURCES {
            match (|| -> Result<Self> {
                let pointer = if let Some(p) = &cached {
                    p.clone()
                } else {
                    net.get(&format!("{base}/engine/{id}.json"))?.0
                };
                let signed = pointer["signed"]
                    .as_str()
                    .ok_or("engine manifest has no signed body")?;
                verify(
                    signed.as_bytes(),
                    pointer["sig"].as_str().ok_or("unsigned engine manifest")?,
                )?;
                let manifest: Value = serde_json::from_str(signed)?;
                if manifest["engine"] != id || manifest["abi"] != 2 {
                    return Err("engine identity or ABI does not match".into());
                }
                let lib = file(net, base, &cache, &manifest["files"][&target])?;
                let data = file(net, base, &cache, &manifest["data"])?;
                state::write(&manifest_path, &pointer)?;
                // SAFETY: the signed official library is verified before loading. Its
                // documented ABI owns returned buffers until wfsim_free is called.
                unsafe {
                    let library = std::mem::ManuallyDrop::new(libloading::Library::new(lib)?);
                    let api: Api = *library.get(b"wfsim_api")?;
                    let free: Free = *library.get(b"wfsim_free")?;
                    let install: Install = *library.get(b"wfsim_install_data")?;
                    let pack = std::fs::read(data)?;
                    let mut len = 0;
                    let p = install(pack.as_ptr(), pack.len(), &mut len);
                    let error = buffer(p, len, free)?;
                    if !error.is_empty() {
                        return Err(format!("engine data: {error}").into());
                    }
                    Ok(Self {
                        id: id.into(),
                        api,
                        free,
                        _library: library,
                    })
                }
            })() {
                Ok(engine) => {
                    if let Err(error) = prune(&cache, &manifest_path) {
                        eprintln!("Engine cache cleanup: {error}");
                    }
                    return Ok(engine);
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        Err(format!(
            "cannot load the official {target} engine: {}",
            errors.join("; ")
        )
        .into())
    }

    pub fn call(&self, path: &str, body: &Value) -> Result<Value> {
        let text = serde_json::to_string(body)?;
        let mut len = 0;
        // SAFETY: slices stay valid for the call; the library outlives this
        // object and its callers, and releases its own returned allocation.
        let out = unsafe {
            buffer(
                (self.api)(
                    path.as_ptr(),
                    path.len(),
                    text.as_ptr(),
                    text.len(),
                    &mut len,
                ),
                len,
                self.free,
            )?
        };
        let value: Value = serde_json::from_str(&out)?;
        if value["ok"] == false || value.get("error").is_some() {
            return Err(format!("engine {path}: {}", value.get("error").unwrap_or(&value)).into());
        }
        Ok(value)
    }
}

unsafe fn buffer(p: *mut u8, len: usize, free: Free) -> Result<String> {
    if p.is_null() {
        return Err("engine returned a null buffer".into());
    }
    // SAFETY: caller supplies an allocation from the official ABI and its free function.
    let bytes = unsafe { std::slice::from_raw_parts(p, len) };
    let text = String::from_utf8(bytes.to_vec());
    unsafe {
        free(p, len);
    }
    Ok(text?)
}

fn file(net: &Net, base: &str, cache: &Path, item: &Value) -> Result<std::path::PathBuf> {
    let path = item["path"]
        .as_str()
        .ok_or("no native engine for this platform yet")?;
    let hash = item["sha256"].as_str().ok_or("missing file hash")?;
    let size = item["size"].as_u64().ok_or("missing file size")?;
    if hash.len() != 64
        || !hash.bytes().all(|c| c.is_ascii_hexdigit())
        || size > 128 * 1024 * 1024
        || !path.starts_with("engine/")
        || path.split('/').any(|c| c == "..")
        || path.contains(['\\', '?', '#'])
    {
        return Err("invalid signed file descriptor".into());
    }
    let ext = path
        .rsplit('.')
        .next()
        .filter(|x| x.bytes().all(|c| c.is_ascii_alphanumeric()))
        .ok_or("invalid file extension")?;
    let dest = cache.join(format!("{hash}.{ext}"));
    let valid = |b: &[u8]| b.len() as u64 == size && format!("{:x}", Sha256::digest(b)) == hash;
    if std::fs::read(&dest).is_ok_and(|b| valid(&b)) {
        return Ok(dest);
    }
    let bytes = net.bytes(&format!("{base}/{path}"), size)?;
    if !valid(&bytes) {
        return Err("engine file hash/size mismatch; refusing to load".into());
    }
    let temporary = dest.with_extension(format!("{}.download", state::random_hex(8)?));
    if let Err(error) =
        std::fs::write(&temporary, bytes).and_then(|_| std::fs::rename(&temporary, &dest))
    {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(dest)
}

fn hex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 {
        return None;
    }
    let bytes: Option<Vec<u8>> = s
        .as_bytes()
        .chunks_exact(2)
        .map(|p| {
            std::str::from_utf8(p)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
        })
        .collect();
    bytes?.try_into().ok()
}

pub fn verify(body: &[u8], sig: &str) -> Result<()> {
    verify_with_keys(body, sig, trust::PUBLIC_KEYS)
}

pub(crate) fn verify_with_keys(body: &[u8], sig: &str, keys: &[&str]) -> Result<()> {
    let signature = Signature::from_bytes(&hex::<64>(sig).ok_or("malformed engine signature")?);
    if keys.iter().any(|key| {
        hex::<32>(key)
            .and_then(|k| VerifyingKey::from_bytes(&k).ok())
            .is_some_and(|k| k.verify(body, &signature).is_ok())
    }) {
        Ok(())
    } else {
        Err("engine signature mismatch; refusing to load".into())
    }
}

fn prune(cache: &Path, current: &Path) -> Result<()> {
    use std::collections::HashSet;
    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(cache)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file() && name.starts_with("manifest.") && name.ends_with(".json")
        {
            manifests.push((entry.path(), entry.metadata()?.modified()?));
        }
    }
    manifests.sort_by(|a, b| {
        (b.0 == current)
            .cmp(&(a.0 == current))
            .then_with(|| b.1.cmp(&a.1))
    });
    let mut keep = HashSet::new();
    for (path, _) in manifests.iter().take(2) {
        let pointer: Value = state::read(path)?;
        let body: Value = serde_json::from_str(
            pointer["signed"]
                .as_str()
                .ok_or("invalid cached manifest")?,
        )?;
        for file in body["files"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.values())
            .chain(std::iter::once(&body["data"]))
        {
            if let Some(sha) = file["sha256"].as_str() {
                keep.insert(sha.to_string());
            }
        }
    }
    for entry in std::fs::read_dir(cache)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type()?.is_file() {
            continue;
        }
        if let Some((hash, _)) = name.split_once('.') {
            if hash.len() == 64
                && hash.bytes().all(|b| b.is_ascii_hexdigit())
                && !keep.contains(hash)
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    for (path, _) in manifests.iter().skip(2) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsigned_and_malformed_manifests() {
        assert!(verify(b"{}", &"00".repeat(64)).is_err());
        assert!(verify(b"{}", "bad").is_err());
        assert!(hex::<2>("00ff").is_some());
        assert!(hex::<2>("00fg").is_none());
    }

    #[test]
    #[ignore = "requires a locally built native library and the WASM quick reference"]
    fn native_compute_matches_the_wasm_reference() -> Result<()> {
        use std::sync::{mpsc, Arc};
        let library_path = std::env::var("WFSIM_TEST_NATIVE")?;
        let data = std::fs::read(std::env::var("WFSIM_TEST_DATA")?)?;
        // SAFETY: this ignored conformance test explicitly loads its caller's
        // local build. Production loading always verifies the official signature.
        let engine = unsafe {
            let library = std::mem::ManuallyDrop::new(libloading::Library::new(library_path)?);
            let api: Api = *library.get(b"wfsim_api")?;
            let free: Free = *library.get(b"wfsim_free")?;
            let install: Install = *library.get(b"wfsim_install_data")?;
            let mut len = 0;
            let p = install(data.as_ptr(), data.len(), &mut len);
            let error = buffer(p, len, free)?;
            if !error.is_empty() {
                return Err(error.into());
            }
            Arc::new(Engine {
                id: "local-conformance".into(),
                api,
                free,
                _library: library,
            })
        };
        let budget = Arc::new(crate::budget::Budget::new(2));
        budget.set(100.0);
        let marker = "__wfsim_cli_capability_probe__";
        assert!(engine
            .call(
                "/api/compute/optimize",
                &serde_json::json!({"weapon":marker})
            )
            .unwrap_err()
            .to_string()
            .contains(marker));
        let (tx, _rx) = mpsc::channel();
        let work: Value = serde_json::from_str(include_str!("../../tests/client_search.json"))?;
        let got = crate::compute::search(&engine, &budget, &work, &tx)?;
        let expected: Value =
            state::read(Path::new(&std::env::var("WFSIM_TEST_SEARCH_EXPECTED")?))?;
        for key in ["build", "score", "work", "counts"] {
            assert_eq!(got[key], expected[key], "native quick {key}");
        }
        let rows: Vec<Value> = serde_json::from_str(include_str!("../../tests/gas/rows.json"))?;
        let row = crate::compute::row(&engine, &budget, &rows[1], &tx)?;
        let expected: Value = state::read(Path::new(&std::env::var("WFSIM_TEST_ROW_EXPECTED")?))?;
        for key in ["score", "metric", "work", "counts"] {
            assert_eq!(row[key], expected["result"][key], "native row {key}");
        }
        Ok(())
    }
}
