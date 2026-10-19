//! THE NATIVE ENGINE, loaded from the content and called by the page.
//!
//! The release carries the engine as a Windows library (`native/engine.json`
//! names it, its engine id and its SHA-256 — docs/DISTRIBUTION.md), so it
//! updates as content does; on a Mac or Linux the shell takes it from the
//! official engine library, as any other client does (`official`). Either way
//! this shell only loads and calls it, by the functions of its C ABI. The page
//! decides whether to use it: it proves it against the wasm first
//! (`82-desktop.js`), and an engine it was not built with is refused here.
//!
//! LOADED FROM A COPY OUTSIDE `current/`, named by its hash: a library Windows
//! has mapped cannot be moved, and `current/` is renamed by every update.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

use sha2::{Digest, Sha256};

type ApiFn = unsafe extern "C" fn(*const u8, usize, *const u8, usize, *mut usize) -> *mut u8;
type FreeFn = unsafe extern "C" fn(*mut u8, usize);
type InstallFn = unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8;

struct Library {
    api: ApiFn,
    free: FreeFn,
    /// Never unloaded: a call may still be running on it.
    _lib: libloading::Library,
}

impl Library {
    fn call(&self, path: &str, body: &str) -> String {
        let mut len = 0usize;
        // SAFETY: the C ABI of `native/src/lib.rs` — two UTF-8 slices in, a
        // buffer out that only `wfsim_free` may release, with its length.
        unsafe {
            let p = (self.api)(path.as_ptr(), path.len(), body.as_ptr(), body.len(), &mut len);
            let out = String::from_utf8_lossy(std::slice::from_raw_parts(p, len)).into_owned();
            (self.free)(p, len);
            out
        }
    }
}

type Job = Box<dyn FnOnce() + Send>;

/// Where one engine's library and data pack are read from, each with the
/// SHA-256 it must have.
struct Named {
    lib: (PathBuf, String),
    data: Option<(PathBuf, String)>,
}

/// One download into the cache at a time, and no cleanup beside one: two lanes
/// asking for a new engine at once fetch it once.
static FETCHING: Mutex<()> = Mutex::new(());

/// THE OFFICIAL ENGINE LIBRARY, where a shell whose content carries no library
/// for its system gets one: the release carries a Windows library only, and
/// every other client of the site reads this (docs/CLIENTS.md §"The official
/// engine library"). The signature first, then each file's SHA-256; files land
/// in the cache under their hash, so a second launch downloads nothing.
fn official(cache: &Path, engine: &str) -> Result<Named, String> {
    let _one = FETCHING.lock();
    let base = crate::update::DEFAULT_SOURCES[0];
    let target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let pointer: serde_json::Value = serde_json::from_slice(&crate::update::fetch(&format!("{base}/engine/{engine}.json"))?)
        .map_err(|e| format!("engine/{engine}.json: {e}"))?;
    let (signed, sig) = match (pointer["signed"].as_str(), pointer["sig"].as_str()) {
        (Some(b), Some(s)) => (b, s),
        _ => return Err(format!("engine/{engine}.json is not signed")),
    };
    crate::update::verify(signed.as_bytes(), sig)?;
    let body: serde_json::Value = serde_json::from_str(signed).map_err(|e| e.to_string())?;
    if body["engine"].as_str() != Some(engine) {
        return Err(format!("engine/{engine}.json names engine {}", body["engine"]));
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let file = |v: &serde_json::Value| -> Result<(PathBuf, String), String> {
        let (path, sha) = match (v["path"].as_str(), v["sha256"].as_str()) {
            (Some(p), Some(s)) if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) => (p, s),
            _ => return Err(format!("engine/{engine}.json names a file without its hash")),
        };
        let ext = path.rsplit('.').next().filter(|e| e.bytes().all(|b| b.is_ascii_alphanumeric())).unwrap_or("bin");
        let local = cache.join(format!("{sha}.{ext}"));
        if std::fs::read(&local).map(|b| format!("{:x}", Sha256::digest(&b)) == sha).unwrap_or(false) {
            return Ok((local, sha.to_string()));
        }
        let bytes = crate::update::fetch(&format!("{base}/{path}"))?;
        if format!("{:x}", Sha256::digest(&bytes)) != sha {
            return Err(format!("{path} is not the file engine/{engine}.json names"));
        }
        std::fs::write(&local, &bytes).map_err(|e| e.to_string())?;
        Ok((local, sha.to_string()))
    };
    let lib = &body["files"][&target];
    if !lib.is_object() {
        return Err(format!("the official engine library has no {target} build of {engine}"));
    }
    let lib = file(lib)?;
    let data = if body["data"].is_object() { Some(file(&body["data"])?) } else { None };
    Ok(Named { lib, data })
}

/// The engines this process has loaded, by engine id, and the threads every
/// call runs on — one per core, so the page's lanes compute in parallel and
/// never on the thread that draws the window.
pub struct Native {
    current: PathBuf,
    cache: PathBuf,
    loaded: Mutex<HashMap<String, Arc<Library>>>,
    jobs: Mutex<mpsc::Sender<Job>>,
}

impl Native {
    pub fn new(current: PathBuf, root: &Path) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        for i in 0..threads {
            let rx = rx.clone();
            // THE ENGINE RECURSES, and a spawned thread's stack is 2 MB by default.
            let _ = std::thread::Builder::new().name(format!("engine-{i}")).stack_size(16 << 20).spawn(move || loop {
                let job = match rx.lock() {
                    Ok(r) => r.recv(),
                    Err(_) => return,
                };
                match job {
                    Ok(job) => job(),
                    Err(_) => return,
                }
            });
        }
        Self { current, cache: root.join("native"), loaded: Mutex::new(HashMap::new()), jobs: Mutex::new(tx) }
    }

    /// The library `current/native/engine.json` names, when its engine id is
    /// `engine`, and its data pack.
    fn content_engine(&self, engine: &str) -> Result<Named, String> {
        let dir = self.current.join("native");
        let named: serde_json::Value = std::fs::read(dir.join("engine.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or("this copy of the app carries no native engine")?;
        if named["engine"].as_str() != Some(engine) {
            return Err(format!("the native engine is {}, the page asked for {engine}", named["engine"]));
        }
        let file = |name: &str, sha: &str| (!name.contains(['/', '\\'])).then(|| (dir.join(name), sha.to_string()));
        let lib = match (named["file"].as_str(), named["sha256"].as_str()) {
            (Some(f), Some(s)) => file(f, s).ok_or("native/engine.json names no file")?,
            _ => return Err("native/engine.json names no file".into()),
        };
        let data = match (named["data"].as_str(), named["data_sha256"].as_str()) {
            (Some(d), Some(s)) => Some(file(d, s).ok_or("native/engine.json names no data file")?),
            _ => None,
        };
        Ok(Named { lib, data })
    }

    /// The engine `engine` as `named` gives it; loaded once, each file's bytes
    /// checked against the SHA-256 named. Held under the lock throughout, so two
    /// lanes never write the copy at once.
    fn load(&self, engine: &str, named: Named) -> Result<Arc<Library>, String> {
        let mut loaded = self.loaded.lock().map_err(|e| e.to_string())?;
        if let Some(l) = loaded.get(engine) {
            return Ok(l.clone());
        }
        let (file, sha) = &named.lib;
        let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
        if format!("{:x}", Sha256::digest(&bytes)) != *sha {
            return Err(format!("{} is not the file its engine names", file.display()));
        }
        std::fs::create_dir_all(&self.cache).map_err(|e| e.to_string())?;
        let copy = self.cache.join(format!("{sha}.{}", std::env::consts::DLL_EXTENSION));
        if *file != copy && std::fs::read(&copy).map(|b| format!("{:x}", Sha256::digest(&b)) != *sha).unwrap_or(true) {
            std::fs::write(&copy, &bytes).map_err(|e| e.to_string())?;
        }
        // SAFETY: a library this release shipped and whose hash it signed; its
        // initialisers are Rust's own.
        let lib = unsafe { libloading::Library::new(&copy) }.map_err(|e| e.to_string())?;
        let l = unsafe {
            let api = *lib.get::<ApiFn>(b"wfsim_api\0").map_err(|e| e.to_string())?;
            let free = *lib.get::<FreeFn>(b"wfsim_free\0").map_err(|e| e.to_string())?;
            // ITS DATA, BEFORE THE FIRST CALL (abi 2, docs/CLIENTS.md §"The
            // official data pack"): the pack the engine names, checked as the
            // library is. A library without the door carries its data.
            if let Some((data, sha)) = &named.data {
                let pack = std::fs::read(data).map_err(|e| format!("{}: {e}", data.display()))?;
                if format!("{:x}", Sha256::digest(&pack)) != *sha {
                    return Err(format!("{} is not the file its engine names", data.display()));
                }
                if let Ok(install) = lib.get::<InstallFn>(b"wfsim_install_data\0") {
                    let mut len = 0usize;
                    let p = install(pack.as_ptr(), pack.len(), &mut len);
                    let said = String::from_utf8_lossy(std::slice::from_raw_parts(p, len)).into_owned();
                    free(p, len);
                    if !said.is_empty() {
                        return Err(format!("{}: {said}", data.display()));
                    }
                }
            }
            Arc::new(Library { api, free, _lib: lib })
        };
        // The copies of engines no longer shipped; one still mapped stays put,
        // and so does the data pack beside it, read again on the next launch.
        let _one = FETCHING.lock();
        if let Ok(dir) = std::fs::read_dir(&self.cache) {
            let keep = |p: &Path| p == copy || named.data.as_ref().is_some_and(|(d, _)| d == p);
            for e in dir.flatten().filter(|e| !keep(&e.path())) {
                let _ = std::fs::remove_file(e.path());
            }
        }
        loaded.insert(engine.to_string(), l.clone());
        Ok(l)
    }

    /// One call of the engine's door, on a pool thread.
    pub async fn call(&self, engine: String, path: String, body: String) -> Result<String, String> {
        let held = self.loaded.lock().map_err(|e| e.to_string())?.get(&engine).cloned();
        let lib = match held {
            Some(l) => l,
            None => {
                let named = if cfg!(windows) {
                    self.content_engine(&engine)?
                } else {
                    // A DOWNLOAD, so off the thread that answers the window.
                    let (cache, id) = (self.cache.clone(), engine.clone());
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(official(&cache, &id));
                    });
                    rx.await.map_err(|_| "the engine download stopped".to_string())??
                };
                self.load(&engine, named)?
            }
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move || {
            let _ = tx.send(lib.call(&path, &body));
        });
        self.jobs.lock().map_err(|e| e.to_string())?.send(job).map_err(|e| e.to_string())?;
        rx.await.map_err(|_| "the native engine stopped".to_string())
    }
}
