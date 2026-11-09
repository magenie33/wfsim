// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::state::{self, Result};
use serde_json::{json, Value};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub const ENGINE_CHANGED: i32 = 75;

pub fn run(dir: &Path, quit: Arc<AtomicBool>) -> Result<()> {
    let nonce = state::random_hex(12)?;
    state::write(
        &dir.join("status.json"),
        &json!({"run":nonce,"pid":std::process::id(),"at":state::timestamp(),"state":"starting"}),
    )?;
    loop {
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--data-dir")
            .arg(dir)
            .args(["join", "--worker"])
            .env("WFSIM_CLI_RUN", &nonce)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        loop {
            let stopped = state::read::<Value>(&dir.join("stop.json"))
                .ok()
                .is_some_and(|s| s["run"] == nonce);
            if quit.load(Ordering::Relaxed) {
                state::write(&dir.join("stop.json"), &json!({"run":nonce}))?;
            }
            if let Some(status) = child.try_wait()? {
                if status.code() == Some(ENGINE_CHANGED)
                    && !quit.load(Ordering::Relaxed)
                    && !stopped
                {
                    println!("Engine update ready; restarting computation.");
                    break;
                }
                if status.success() || quit.load(Ordering::Relaxed) || stopped {
                    return Ok(());
                }
                return Err(format!("compute process exited with {status}").into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
