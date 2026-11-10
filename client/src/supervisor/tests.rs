// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    sync::Mutex,
};

fn until(mut ready: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(25);
    while !ready() {
        assert!(Instant::now() < until, "host lifecycle condition timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}

struct StopOnDrop(Arc<AtomicBool>, PathBuf);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
        if let Ok(status) = state::read::<Value>(&self.1.join("status.json")) {
            let _ = state::write(&self.1.join("stop.json"), &json!({"run":status["run"]}));
        }
    }
}

#[test]
#[ignore = "build the client and set WFSIM_CLI_TEST_BINARY; all traffic stays on loopback"]
fn host_update_restarts_the_real_worker_and_rolls_back_a_crash() -> Result<()> {
    scenario("WFSIM_CLI_TEST_BINARY", false)
}

#[test]
#[ignore = "requires the isolated startup-failure executable from check_client_update.mjs"]
fn host_update_rolls_back_a_clean_exit_without_startup_acknowledgment() -> Result<()> {
    scenario("WFSIM_CLI_TEST_BAD_BINARY", true)
}

fn scenario(binary_variable: &str, fails_before_ready: bool) -> Result<()> {
    let binary =
        PathBuf::from(std::env::var_os(binary_variable).ok_or("set the candidate binary path")?);
    let base_binary = PathBuf::from(
        std::env::var_os("WFSIM_CLI_TEST_BASE_BINARY").ok_or("set WFSIM_CLI_TEST_BASE_BINARY")?,
    );
    let build_version = |path: &Path| -> Result<Version> {
        let output = Command::new(path)
            .arg("--version")
            .env("WFSIM_CLI_NO_DELEGATE", "1")
            .output()?;
        Ok(Version::parse(
            String::from_utf8(output.stdout)?
                .trim()
                .strip_prefix("wfsim ")
                .ok_or("missing build version")?,
        )?)
    };
    let next_version = build_version(&binary)?;
    let base_version = build_version(&base_binary)?;
    assert!(next_version > base_version);
    let next_name = next_version.to_string();
    let dir = std::env::temp_dir().join(format!("wfsim-host-live-{}", state::random_hex(8)?));
    state::directory(&dir)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let server = format!("http://{}", listener.local_addr()?);
    let bytes = fs::read(&binary)?;
    let target = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let body = serde_json::to_string(
        &json!({"v":1,"launch":1,"version":next_name,"npm":true,"files":{target.clone():{
        "name":format!("wfsim-{}{}",target,if cfg!(windows){".exe"}else{""}),
        "size":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes)),"urls":[format!("{server}/native")]}}}),
    )?;
    let key = SigningKey::from_bytes(&[9; 32]);
    let signature = key
        .sign(body.as_bytes())
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let public = key
        .verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let enabled = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let (enable, done, identities) = (enabled.clone(), finished.clone(), seen.clone());
    let serving_version = next_name.clone();
    let serving = std::thread::spawn(move || {
        while !done.load(Ordering::Relaxed) {
            let (mut client, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            client.set_nonblocking(false).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 8192];
            let (path, content) = loop {
                let n = client.read(&mut chunk).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&chunk[..n]);
                if let Some(end) = request.windows(4).position(|p| p == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break (
                            headers.split_whitespace().nth(1).unwrap().to_string(),
                            request[end + 4..end + 4 + length].to_vec(),
                        );
                    }
                }
            };
            let response = if path == "/cli/latest.json" {
                serde_json::to_vec(&json!({"version":if enable.load(Ordering::Relaxed){serving_version.as_str()}else{"0.1.0"}})).unwrap()
            } else if path.ends_with("release.json.sig") {
                signature.as_bytes().to_vec()
            } else if path.ends_with("release.json") {
                body.as_bytes().to_vec()
            } else if path == "/native" {
                bytes.clone()
            } else if path == "/api/board/sync" {
                let sent: Value = serde_json::from_slice(&content).unwrap();
                identities
                    .lock()
                    .unwrap()
                    .push(sent["verifier"].as_str().unwrap().to_string());
                serde_json::to_vec(&json!({"ok":true,"taken":[],"dropped":[],"held":[],"pack":null,"work":null,"next_ms":1000})).unwrap()
            } else {
                panic!("unexpected endpoint {path}")
            };
            write!(
                client,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len()
            )
            .unwrap();
            let _ = client.write_all(&response);
        }
    });
    let config = Config {
        server: server.clone(),
        range: crate::state::Range { min: 0, max: 0 },
        name: "Host update fixture".into(),
        paused: false,
        consent: Some(crate::state::Consent {
            v: 1,
            at: state::timestamp(),
        }),
    };
    config.save(&dir)?;
    let identity = state::identity(&dir)?;
    let pending = json!([{"engine":"fixture","path":"/api/board/sync","body":{"question":"retained","lease":"a".repeat(32),"score":1.2345678901234567,"work":100}}]);
    state::write(&dir.join("pending.json"), &pending)?;
    let saved_config = fs::read(dir.join("config.json"))?;
    let quit = Arc::new(AtomicBool::new(false));
    let _cleanup = StopOnDrop(quit.clone(), dir.clone());
    let (directory, stop) = (dir.clone(), quit.clone());
    let supervising = std::thread::spawn(move || {
        run_with(
            &directory,
            stop,
            base_binary,
            base_version,
            vec![server],
            vec![public],
            Duration::from_millis(100),
        )
    });
    let status = || state::read::<Value>(&dir.join("host-ready.json")).unwrap_or(Value::Null);
    until(|| status()["pid"].as_u64().is_some());
    let first = status();
    let mut before = first.clone();
    enabled.store(true, Ordering::Relaxed);
    if !fails_before_ready {
        until(|| {
            update::installed(&dir)
                .ok()
                .is_some_and(|s| s.current.is_some())
                && status()["pid"] != first["pid"]
        });
        let upgraded = status();
        before = upgraded.clone();
        assert_eq!(first["run"], upgraded["run"]);
        assert_eq!(fs::read_to_string(dir.join("identity"))?, identity);
        assert_eq!(fs::read(dir.join("config.json"))?, saved_config);
        assert_eq!(state::read::<Value>(&dir.join("pending.json"))?, pending);
        assert!(seen.lock().unwrap().iter().all(|id| id == &identity));
        let pid = upgraded["pid"].as_u64().unwrap() as u32;
        #[cfg(unix)]
        {
            // SAFETY: this is the exact child PID observed in this test's private directory.
            assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
        }
        #[cfg(windows)]
        assert!(Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .status()?
            .success());
    }
    until(|| {
        update::installed(&dir).ok().is_some_and(|s| {
            s.current.is_none() && s.highest.as_deref() == Some(next_name.as_str())
        }) && status()["pid"] != before["pid"]
    });
    assert_eq!(state::read::<Value>(&dir.join("pending.json"))?, pending);
    let restored = status();
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(
        status()["pid"],
        restored["pid"],
        "a rejected release must not cause a restart loop"
    );
    state::write(&dir.join("stop.json"), &json!({"run":restored["run"]}))?;
    assert!(supervising.join().unwrap().is_ok());
    finished.store(true, Ordering::Relaxed);
    serving.join().unwrap();
    assert_eq!(fs::read_to_string(dir.join("identity"))?, identity);
    assert_eq!(state::read::<Value>(&dir.join("pending.json"))?, pending);
    fs::remove_dir_all(dir)?;
    Ok(())
}
