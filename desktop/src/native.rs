//! THE NATIVE ENGINE, loaded from the content and called by the page.
//!
//! The release carries the engine as a library (`native/engine.json` names it,
//! its engine id and its SHA-256 — docs/DISTRIBUTION.md), so it updates as
//! content does and this shell only loads and calls it, by the two functions of
//! its C ABI. The page decides whether to use it: it proves it against the wasm
//! first (`82-desktop.js`), and an engine it was not built with is refused here.
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
    /// `engine`; loaded once, its bytes checked against the SHA-256 named there.
    /// Held under the lock throughout, so two lanes never write the copy at once.
    fn library(&self, engine: &str) -> Result<Arc<Library>, String> {
        let mut loaded = self.loaded.lock().map_err(|e| e.to_string())?;
        if let Some(l) = loaded.get(engine) {
            return Ok(l.clone());
        }
        let named: serde_json::Value = std::fs::read(self.current.join("native").join("engine.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or("this copy of the app carries no native engine")?;
        if named["engine"].as_str() != Some(engine) {
            return Err(format!("the native engine is {}, the page asked for {engine}", named["engine"]));
        }
        let (file, sha) = match (named["file"].as_str(), named["sha256"].as_str()) {
            (Some(f), Some(s)) if !f.contains(['/', '\\']) => (f, s),
            _ => return Err("native/engine.json names no file".into()),
        };
        let bytes = std::fs::read(self.current.join("native").join(file)).map_err(|e| format!("{file}: {e}"))?;
        if format!("{:x}", Sha256::digest(&bytes)) != sha {
            return Err(format!("{file} is not the file native/engine.json names"));
        }
        std::fs::create_dir_all(&self.cache).map_err(|e| e.to_string())?;
        let copy = self.cache.join(format!("{sha}.dll"));
        if std::fs::read(&copy).map(|b| format!("{:x}", Sha256::digest(&b)) != sha).unwrap_or(true) {
            std::fs::write(&copy, &bytes).map_err(|e| e.to_string())?;
        }
        // SAFETY: a library this release shipped and whose hash it signed; its
        // initialisers are Rust's own.
        let lib = unsafe { libloading::Library::new(&copy) }.map_err(|e| e.to_string())?;
        let l = unsafe {
            let api = *lib.get::<ApiFn>(b"wfsim_api\0").map_err(|e| e.to_string())?;
            let free = *lib.get::<FreeFn>(b"wfsim_free\0").map_err(|e| e.to_string())?;
            // ITS DATA, BEFORE THE FIRST CALL (abi 2, docs/CLIENTS.md §"The
            // official data pack"): the pack `engine.json` names, checked as the
            // library is. A library without the door carries its data.
            if let (Some(data), Some(sha)) = (named["data"].as_str(), named["data_sha256"].as_str()) {
                if data.contains(['/', '\\']) {
                    return Err("native/engine.json names no data file".into());
                }
                let pack = std::fs::read(self.current.join("native").join(data)).map_err(|e| format!("{data}: {e}"))?;
                if format!("{:x}", Sha256::digest(&pack)) != sha {
                    return Err(format!("{data} is not the file native/engine.json names"));
                }
                if let Ok(install) = lib.get::<InstallFn>(b"wfsim_install_data\0") {
                    let mut len = 0usize;
                    let p = install(pack.as_ptr(), pack.len(), &mut len);
                    let said = String::from_utf8_lossy(std::slice::from_raw_parts(p, len)).into_owned();
                    free(p, len);
                    if !said.is_empty() {
                        return Err(format!("{data}: {said}"));
                    }
                }
            }
            Arc::new(Library { api, free, _lib: lib })
        };
        // The copies of engines no longer shipped; one still mapped stays put.
        if let Ok(dir) = std::fs::read_dir(&self.cache) {
            for e in dir.flatten().filter(|e| e.path() != copy) {
                let _ = std::fs::remove_file(e.path());
            }
        }
        loaded.insert(engine.to_string(), l.clone());
        Ok(l)
    }

    /// One call of the engine's door, on a pool thread.
    pub async fn call(&self, engine: String, path: String, body: String) -> Result<String, String> {
        let lib = self.library(&engine)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move || {
            let _ = tx.send(lib.call(&path, &body));
        });
        self.jobs.lock().map_err(|e| e.to_string())?.send(job).map_err(|e| e.to_string())?;
        rx.await.map_err(|_| "the native engine stopped".to_string())
    }
}
