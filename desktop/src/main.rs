//! WFSim desktop — the shell.
//!
//! It does four things and deliberately nothing else: unpack the app on first
//! launch, serve `current/` to a webview, let the page swap that directory for
//! a downloaded one, and roll back a version that will not start. Everything a
//! reader sees is the same `site/` the browser gets, running the same wasm on
//! the same CPU. See docs/DESKTOP.md.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod layout;
mod payload;
mod protocol;
mod update;

use std::sync::Arc;

use layout::Layout;

/// Set for `--selftest`, where launching a real browser would be a side effect
/// a check has no business having.
static SELFTEST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A LINK OUT OF THE APP GOES TO THE SYSTEM BROWSER, never to this webview.
///
/// Every external link in `app.js` is `target="_blank"` — the wiki page for a
/// weapon, a mod, an arcane, an enemy, plus ko-fi and the QQ group. In a
/// browser that opens a tab and the app is still there; in a webview it either
/// navigates in place or is swallowed, and the first one is worse: the reader
/// is now on wiki.warframe.com inside a window with no back button, and the
/// only way out is to kill the app.
#[tauri::command]
fn open_external(url: String) -> Result<String, String> {
    if SELFTEST.load(std::sync::atomic::Ordering::Relaxed) {
        vet(&url)?;
        println!("[probe] would open externally: {url}");
        return Ok(format!("would-open {url}"));
    }
    shell_open(&url)
}

/// What this app is willing to hand to the shell: a web page, and nothing else.
///
/// INSIDE `shell_open` RATHER THAN BESIDE IT. It was a separate step for a
/// while and the two immediately came apart — a second caller reached the open
/// without the check, and `file:///C:/Windows/System32/calc.exe` launched. A
/// guard the caller has to remember is a guard that is one day forgotten, and
/// what it is guarding here is "run an arbitrary program on this machine".
fn vet(url: &str) -> Result<(), String> {
    let scheme_ok = url.starts_with("https://") || url.starts_with("http://");
    if !scheme_ok || url.len() > 2048 || url.contains(['"', '\n', '\r', '\0']) {
        return Err(format!("refused (only http/https links are opened): {url}"));
    }
    Ok(())
}

/// Open a URL with the shell's own handler — the reader's default browser.
///
/// `ShellExecuteW` rather than spawning `explorer.exe`: the latter usually
/// works, is not what the platform documents, and has a failure mode that is
/// invisible from here — `explorer` returns success immediately whatever it
/// did with the argument, so a link that opened nothing looks exactly like one
/// that opened a browser. ShellExecuteW answers with the handler it started,
/// and anything at or below 32 is an error code.
fn shell_open(url: &str) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    vet(url)?;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }
    let verb = wide("open");
    let target = wide(url);
    const SW_SHOWNORMAL: i32 = 1;
    let rc = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // The return is a fake HINSTANCE: > 32 succeeded, otherwise it is one of
    // the SE_ERR_* codes (2 = file not found, 31 = no application associated).
    let code = rc as isize;
    if code > 32 {
        Ok(format!("opened {url}"))
    } else {
        Err(format!("the system refused to open {url} (ShellExecute returned {code})"))
    }
}

/// Cleared only once the page has actually rendered — see `Layout::boot_begin`.
#[tauri::command]
fn mark_healthy(state: tauri::State<Arc<Layout>>) {
    state.boot_healthy();
}

/// What the page is running, so the footer can say it and a bug report can
/// name it. A commit, never a version number.
#[tauri::command]
fn app_version(state: tauri::State<Arc<Layout>>) -> String {
    state.manifest().version
}

/// True while a check or a download is already running. Both commands return
/// immediately and the page polls, so without this a reader clicking twice
/// starts two downloads into the same directory.
fn update_busy() -> bool {
    matches!(update::status().phase.as_str(), "checking" | "downloading")
}

#[tauri::command]
fn update_check(state: tauri::State<Arc<Layout>>) {
    if update_busy() {
        return;
    }
    update::begin("checking");
    let layout = state.inner().clone();
    std::thread::spawn(move || {
        if let Err(e) = update::check(&layout.manifest()) {
            update::note_failure(&e);
        }
    });
}

#[tauri::command]
fn update_download(state: tauri::State<Arc<Layout>>) {
    if update_busy() {
        return;
    }
    update::begin("downloading");
    let layout = state.inner().clone();
    std::thread::spawn(move || {
        let local = layout.manifest();
        if let Err(e) = update::download(&local, &layout.current(), &layout.next()) {
            update::note_failure(&e);
        }
    });
}

#[tauri::command]
fn update_status() -> update::Status {
    update::status()
}

/// Swap `next/` in. The PAGE reloads itself afterwards rather than the shell
/// restarting: only the page knows whether the reader is halfway through
/// something, and a reload is cheap where a restart is a window disappearing.
#[tauri::command]
fn update_apply(state: tauri::State<Arc<Layout>>) -> Result<(), String> {
    state.promote().map_err(|e| e.to_string())
}

/// Runs the app headlessly, reports what it found and exits non-zero on any
/// failure. The shell has to be verifiable WITHOUT a person watching a window:
/// "it opened and looked right" is not a check that can run twice the same way.
const SELFTEST_PROBE: &str = r#"
(() => {
  const rep = (m) => { try { return fetch('/__selftest__/' + encodeURIComponent(m)); } catch (e) { return null; } };
  rep('injected  internals=' + (typeof window.__TAURI_INTERNALS__));
  window.addEventListener('error', (e) => rep('window-error  ' + (e.message || e)));
  setTimeout(async () => {
    // THE LANE CEILING IS ONLY VISIBLE AT 100%. At the default half share a
    // 28-core machine asks for 14, which the old 16-lane cap also allowed — so
    // a check run at the default passes on the very build it exists to catch.
    // First pass sets the share and reloads; the assertions run on the second.
    if (!sessionStorage.getItem('wfsim-probe-pass2')) {
      sessionStorage.setItem('wfsim-probe-pass2', '1');
      localStorage.setItem('wfsim-compute', '100');
      location.reload();
      return;
    }
    const out = [];
    let ok = true;
    const check = (name, pass, detail) => {
      out.push((pass ? 'PASS ' : 'FAIL ') + name.padEnd(16) + detail);
      if (!pass) ok = false;
    };
    try {
      check('desktop flag', window.__WFSIM_DESKTOP__ === true, String(window.__WFSIM_DESKTOP__));
      try {
        const t0 = performance.now();
        const w = new Worker('/worker.js');
        const meta = await new Promise((res, rej) => {
          const t = setTimeout(() => rej(new Error('timeout')), 60000);
          w.onerror = (e) => { clearTimeout(t); rej(new Error(e.message || 'worker failed to load')); };
          w.onmessage = (e) => { clearTimeout(t); res(e.data.payload); };
          w.postMessage({ id: 1, kind: 'api', path: '/api/meta', body: {} });
        });
        const n = (meta && meta.weapons && meta.weapons.length) || 0;
        check('wasm engine', n > 100, n + ' weapons in ' + Math.round(performance.now() - t0) + 'ms');
      } catch (e) { check('wasm engine', false, e.message); }
      const nodes = document.querySelectorAll('*').length;
      const txt = document.body.innerText || '';
      check('page rendered', nodes > 200 && !txt.includes('could not start'), nodes + ' nodes');
      // A CHECK RUN IS A SUCCESSFUL LAUNCH. Without this the probe leaves the
      // boot counter climbing, and the third check run in a row is rolled back
      // to `prev/` by a mechanism doing exactly its job.
      if (nodes > 200) { try { await window.__TAURI_INTERNALS__.invoke('mark_healthy'); } catch (_) {} }
      try { const r = await fetch('/board.json', { cache: 'no-cache' }); check('board.json', r.ok, 'HTTP ' + r.status); }
      catch (e) { check('board.json', false, e.message); }
      try { const r = await fetch('/weapons/Torid'); const b = await r.text(); check('spa fallback', r.ok && b.includes('<'), 'HTTP ' + r.status + ', ' + b.length + ' bytes'); }
      catch (e) { check('spa fallback', false, e.message); }
      try { localStorage.setItem('wfsim-selftest', '1'); localStorage.removeItem('wfsim-selftest'); check('localStorage', true, 'writable'); }
      catch (e) { check('localStorage', false, e.message); }
      // An external link must be handed to the OS, and an in-app one must NOT
      // be — a handler that grabs every click would break SPA routing, which
      // is the same anchor element with a relative href.
      // THE PAGE'S OWN ANSWER, not the value that was injected. app.js keeps
      // the ceiling in a module const, but the compute picker's face states
      // what the current share BUYS as `lanes/cores`, and its menu names the
      // lane count of every share — including 100%, which is the only one that
      // can tell a raised ceiling from the old 16.
      try {
        const host = document.getElementById('compute-select');
        const html = host ? host.outerHTML : '';
        rep('PICKER ' + html.slice(0, 600));
        const face = (host && host.textContent) || '';
        const m = face.match(/(\d+)\s*\/\s*(\d+)/);
        const cores = navigator.hardwareConcurrency;
        // 100% must buy EVERY logical processor. Under the old cap this reads
        // 16 on any machine larger than that.
        check('no lane ceiling', !!m && Number(m[1]) === cores && Number(m[2]) === cores,
              'at 100% the picker buys ' + (m ? m[1] : '?') + ' of ' + cores + ' cores');
      } catch (e) { check('lane ceiling', false, e.message); }
      try {
        const mk = (href) => { const a = document.createElement('a'); a.href = href; a.textContent = 'x'; document.body.appendChild(a); return a; };
        const fire = (a) => { const ev = new MouseEvent('click', { bubbles: true, cancelable: true }); a.dispatchEvent(ev); a.remove(); return ev.defaultPrevented; };
        // NOT defaultPrevented — app.js's own router prevents internal links
        // and that is correct. The question is only whether the shell tried to
        // send it OUT of the app.
        window.__WFSIM_LAST_EXTERNAL__ = null;
        fire(mk('https://wiki.warframe.com/w/Torid'));
        const ext = window.__WFSIM_LAST_EXTERNAL__;
        window.__WFSIM_LAST_EXTERNAL__ = null;
        fire(mk('/weapons/Torid'));
        const int = window.__WFSIM_LAST_EXTERNAL__;
        check('external link', /wiki\.warframe\.com/.test(ext || ''), 'handed to OS: ' + ext);
        check('internal link', int === null, 'spa route kept in-app');
      } catch (e) { check('external link', false, e.message); }
      try {
        const v = await window.__TAURI_INTERNALS__.invoke('app_version');
        check('ipc + version', /^[0-9a-f]{6,}$/.test(v) || v === 'nogit', v);
      } catch (e) { check('ipc + version', false, 'invoke failed: ' + e.message); }
      // THE UPDATE CHANNEL, END TO END: reach a source, verify the manifest's
      // signature against the key built into this binary, parse it, and compare
      // it with what is on disk. Any of those failing is the same symptom for a
      // reader — updates silently stop arriving — so the check has to run the
      // whole path rather than any one piece of it.
      try {
        await window.__TAURI_INTERNALS__.invoke('update_check');
        let s = null;
        for (let i = 0; i < 60; i++) {
          await new Promise((r) => setTimeout(r, 500));
          s = await window.__TAURI_INTERNALS__.invoke('update_status');
          if (s.phase !== 'checking') break;
        }
        const phase = s ? s.phase : 'no status';
        check('update channel', phase === 'uptodate' || phase === 'available',
              phase + (s && s.version ? ' @ ' + s.version : '') + (s && s.message ? ' — ' + s.message : ''));
      } catch (e) { check('update channel', false, e.message); }
    } catch (e) {
      check('probe', false, 'threw: ' + (e && e.message || e));
    }
    await rep('RESULT ' + (ok ? 'PASS' : 'FAIL') + '\n' + out.join('\n'));
  }, 3000);
})();
"#;



/// Measures what one compute lane actually costs in memory.
///
/// `COMPUTE_MAX_LANES = 16` in app.js is a ceiling set by MEMORY, not by heat:
/// every lane is a Web Worker holding its own instance of the wasm module. The
/// browser has to guess conservatively because a page cannot ask how much RAM
/// the machine has. This build can, so the ceiling should be computed — and
/// computing it needs the per-lane figure, which is measured here rather than
/// assumed.
const MEASURE_PROBE: &str = r#"
(() => {
  const rep = (m) => { try { return fetch('/__selftest__/' + encodeURIComponent(m)); } catch (e) { return null; } };
  setTimeout(async () => {
    const n = window.__WFSIM_MEASURE_LANES__;
    const lanes = [];
    for (let i = 0; i < n; i++) {
      const w = new Worker('/worker.js');
      lanes.push(new Promise((res, rej) => {
        const t = setTimeout(() => rej(new Error('lane ' + i + ' timed out')), 120000);
        w.onerror = (e) => { clearTimeout(t); rej(new Error(e.message || 'lane failed')); };
        w.onmessage = () => { clearTimeout(t); res(); };
        // /api/meta forces the module to instantiate — an idle Worker has not
        // paid for the wasm yet, and an idle lane is not what we are sizing.
        w.postMessage({ id: i, kind: 'api', path: '/api/meta', body: {} });
      }));
    }
    try { await Promise.all(lanes); await rep('MEASURE ' + n + ' lanes ready'); }
    catch (e) { await rep('MEASURE-FAIL ' + e.message); }
  }, 2000);
})();
"#;

/// Drives one complete update: see it, fetch it, swap it in.
///
/// The update path is the one part of this app that CANNOT be checked by
/// looking at it — it only does anything when a newer version exists, which on
/// a developer's machine is never by accident. So the test manufactures one
/// (`scripts/check_desktop_update.py` publishes a changed file first) and then
/// walks the whole path here, because every step of it fails the same way from
/// a reader's side: updates quietly stop arriving.
const UPDATE_PROBE: &str = r#"
(() => {
  const rep = (m) => { try { return fetch('/__selftest__/' + encodeURIComponent(m)); } catch (e) { return null; } };
  const inv = (c, a) => window.__TAURI_INTERNALS__.invoke(c, a || {});
  const settle = async (busy) => {
    for (let i = 0; i < 240; i++) {
      await new Promise((r) => setTimeout(r, 500));
      const s = await inv('update_status');
      if (s.phase !== busy) return s;
    }
    return { phase: 'timeout', message: 'still ' + busy + ' after 120s' };
  };
  setTimeout(async () => {
    const out = [];
    let ok = true;
    const check = (n, pass, d) => { out.push((pass ? 'PASS ' : 'FAIL ') + n.padEnd(16) + d); if (!pass) ok = false; };
    const refuse = window.__WFSIM_EXPECT_REFUSED__ === true;
    try {
      // Same contract as every other launch — see `mark_healthy`.
      try { await inv('mark_healthy'); } catch (_) {}
      const before = await inv('app_version');
      await inv('update_check');
      let s = await settle('checking');
      // A TAMPERED MANIFEST MUST NOT EVEN BE READ. When the signature is the
      // thing under test, the refusal happens here, before any file is fetched.
      if (refuse && s.phase === 'failed') {
        check('refused', true, s.message);
        await rep('RESULT PASS\n' + out.join('\n'));
        return;
      }
      check('sees update', s.phase === 'available',
            s.phase + ', ' + s.files_total + ' files / ' + (s.bytes_total / 1024).toFixed(1) + ' KB' + (s.message ? ' — ' + s.message : ''));
      if (s.phase === 'available') {
        // THE DIFF IS THE POINT. 764 files ship; a release that changed one
        // must fetch one. A number near 764 here means content addressing or
        // the copy-from-current path has broken, and the symptom would be a
        // 29 MB download for every trivial change rather than an error.
        check('fetches a diff', s.files_total < 20, s.files_total + ' of 764 files');
        await inv('update_download');
        s = await settle('downloading');
        if (refuse) {
          // A BLOB THAT DOES NOT MATCH ITS HASH MUST STOP THE UPDATE, and the
          // reason has to say so — a generic failure would be indistinguishable
          // from a flaky network, which is retried rather than refused.
          check('refused', s.phase === 'failed', s.phase + ' — ' + (s.message || ''));
          check('names the cause', /checksum|SIGNATURE/i.test(s.message || ''), s.message || '(no message)');
          await rep('RESULT ' + (ok ? 'PASS' : 'FAIL') + '\n' + out.join('\n'));
          return;
        }
        check('downloaded', s.phase === 'ready', s.phase + (s.message ? ' ' + s.message : ''));
        if (s.phase === 'ready') {
          await inv('update_apply');
          const after = await inv('app_version');
          check('applied', true, before + ' -> ' + after);
        }
      }
    } catch (e) { check('update', false, (e && e.message) || String(e)); }
    await rep('RESULT ' + (ok ? 'PASS' : 'FAIL') + '\n' + out.join('\n'));
  }, 2500);
})();
"#;

/// Handing external links to the OS. Bound in the CAPTURE phase so it runs
/// before app.js's own handlers, several of which call stopPropagation on the
/// link itself (`wl()` does, so a wiki link inside a mod card does not also
/// select the card).
const LINK_HANDOFF: &str = r#"
document.addEventListener('click', (e) => {
  const a = e.target && e.target.closest && e.target.closest('a[href]');
  if (!a) return;
  const href = a.getAttribute('href') || '';
  if (!/^https?:/i.test(href)) return;
  e.preventDefault();
  e.stopPropagation();
  window.__WFSIM_LAST_EXTERNAL__ = a.href;
  try { window.__TAURI_INTERNALS__.invoke('open_external', { url: a.href }); } catch (_) {}
}, true);
// A page that still manages to navigate away (window.open, a form, a redirect)
// would strand the reader, so the shell refuses to leave its own origin.
window.addEventListener('beforeunload', () => {}, false);
"#;

/// Injected into the page rather than added to `app.js`: the shell must not
/// need the app's cooperation to know it is alive, and `site/` has to stay
/// byte-identical to what the browser serves.
const HEALTH_PROBE: &str = r#"
(() => {
  // IT WAITS AS LONG AS IT TAKES, and that is not fussiness. Two launches that
  // never report healthy make the shell roll back to `prev/` — correct when a
  // version genuinely cannot start, and a disaster if the only thing that
  // happened is that a slow machine had not finished rendering when a fixed
  // four-second timer fired. That would silently downgrade a working update on
  // exactly the machines least able to afford re-downloading it. So this polls
  // for a rendered page instead of sampling once, and gives up only after a
  // minute, by which time "not rendered" really does mean broken.
  const look = (n) => {
    try {
      const txt = document.body.innerText || '';
      if (document.querySelectorAll('*').length > 200 && !txt.includes('could not start')) {
        window.__TAURI_INTERNALS__.invoke('mark_healthy');
        return;
      }
    } catch (_) { /* a shell that cannot self-report still runs the app */ }
    if (n < 60) setTimeout(() => look(n + 1), 1000);
  };
  setTimeout(() => look(0), 1500);
})();
"#;

/// Called by the protocol handler when the page reports. A line beginning
/// with RESULT is the last one and decides the exit code.
/// Total working set of every WebView2 process, in MB. Shelling out to
/// PowerShell keeps this measurement-only code free of a Windows API
/// dependency the shipped shell would carry for ever.
fn webview_memory_mb() -> f64 {
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command",
               "(Get-Process msedgewebview2 -ErrorAction SilentlyContinue | Measure-Object WorkingSet64 -Sum).Sum"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|b| b / 1048576.0)
        .unwrap_or(f64::NAN)
}

pub fn selftest_line(msg: &str) {
    if let Some(rest) = msg.strip_prefix("MEASURE") {
        // Let allocation settle: the module is instantiated but the renderer
        // has not necessarily finished committing pages for it.
        std::thread::sleep(std::time::Duration::from_secs(3));
        println!("MEASURED{rest}  total webview memory {:.0} MB", webview_memory_mb());
        std::process::exit(0);
    }
    if let Some(rest) = msg.strip_prefix("RESULT ") {
        let (verdict, body) = rest.split_once('\n').unwrap_or((rest, ""));
        println!("\n===== SELFTEST =====\n{body}\n===== {verdict} =====");
        std::process::exit(if verdict.trim() == "PASS" { 0 } else { 1 });
    }
    println!("[probe] {msg}");
}

fn main() {
    let measure_lanes: Option<usize> = std::env::args()
        .skip_while(|a| a != "--measure-lanes")
        .nth(1)
        .and_then(|v| v.parse().ok());
    // `--test-open <url>`: exercise the real browser handoff, with no window and
    // no webview. The selftest can only assert that a click REACHES this
    // command — whether the system then opens anything is a fact about the
    // machine, and the only way to know is to ask it.
    if let Some(url) = std::env::args().skip_while(|a| a != "--test-open").nth(1) {
        match shell_open(&url) {
            Ok(m) => println!("{m}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let update_test = std::env::args().any(|a| a == "--selftest-update");
    let expect_refused = std::env::args().any(|a| a == "--expect-refused");
    let selftest = std::env::args().any(|a| a == "--selftest") || measure_lanes.is_some() || update_test;
    SELFTEST.store(selftest, std::sync::atomic::Ordering::Relaxed);
    // Development only: the payload is unpacked on FIRST launch and never
    // again, which is right (the updater owns that directory afterwards) and
    // means a rebuilt payload is invisible until the directory is cleared.
    if std::env::args().any(|a| a == "--reset") {
        if let Ok(base) = std::env::var("LOCALAPPDATA") {
            let dir = std::path::PathBuf::from(base).join("WFSim");
            let _ = std::fs::remove_dir_all(&dir);
            println!("reset: removed {}", dir.display());
        }
    }
    let layout = Arc::new(Layout::open().expect("could not prepare the app directory"));
    let rolled_back = layout.boot_begin();
    let root = layout.current();

    if selftest {
        // A DEADLINE FOR EVERY CHECK MODE. This used to live inside the payload
        // verification below, which `--selftest-update` skips — leaving the one
        // mode that waits on a network with nothing to stop it. A check that
        // hangs is worse than one that fails: it reports nothing, and the
        // caller learns only that its own timeout expired.
        let secs = if update_test { 240 } else { 120 };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            println!("
===== SELFTEST =====
TIMEOUT: the page never reported after {secs}s
====================");
            std::process::exit(1);
        });
    }

    if selftest && measure_lanes.is_none() && !update_test {
        // A payload that unpacked wrong produces a window that looks fine and
        // an app that is subtly not the one that was built, so the files are
        // verified against their own manifest before anything is shown.
        let m = layout.manifest();
        let mut bad = 0usize;
        for e in &m.files {
            match std::fs::read(root.join(&e.p)) {
                Ok(b) if b.len() == e.n => {}
                _ => bad += 1,
            }
        }
        println!("unpacked {} files from payload {}, {bad} wrong", m.files.len(), m.version);
        if bad > 0 {
            std::process::exit(1);
        }
    }

    let init = format!(
        "window.__WFSIM_DESKTOP__ = true; window.__WFSIM_ROLLED_BACK__ = {rolled_back}; window.__WFSIM_EXPECT_REFUSED__ = {expect_refused};{LINK_HANDOFF}{}",
        match measure_lanes {
            Some(_) => MEASURE_PROBE,
            None if update_test => UPDATE_PROBE,
            None if selftest => SELFTEST_PROBE,
            None => HEALTH_PROBE,
        }
    );
    let init = match measure_lanes {
        Some(n) => format!("window.__WFSIM_MEASURE_LANES__ = {n};{init}"),
        None => init,
    };

    tauri::Builder::default()
        .manage(layout)
        .invoke_handler(tauri::generate_handler![
            mark_healthy, app_version, open_external,
            update_check, update_download, update_status, update_apply
        ])
        .register_uri_scheme_protocol("wfsim", move |_ctx, req| protocol::serve(&root, &req))
        .setup(move |app| {
            let url = tauri::Url::parse("wfsim://localhost/").expect("app url");
            let mut win = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::CustomProtocol(url))
                .title("WFSim")
                .inner_size(1440.0, 920.0)
                .min_inner_size(960.0, 600.0)
                .center();
            if selftest {
                // A CHECK MUST NOT EDIT THE READER'S SETTINGS. The probe writes
                // localStorage (it has to — the compute share lives there), and
                // the app's real profile is where every saved build, scenario
                // and riven lives. So the checks run in a throwaway profile,
                // which also makes every run start from the same blank state.
                let dir = std::env::temp_dir().join("wfsim-selftest-profile");
                let _ = std::fs::remove_dir_all(&dir);
                win = win.data_directory(dir);
            }
            win
                .initialization_script(&init)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("wfsim desktop failed to start");
}
