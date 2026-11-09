// SPDX-License-Identifier: AGPL-3.0-or-later
mod budget;
mod compute;
mod engine;
mod net;
mod runner;
mod state;
mod supervisor;

use clap::{Parser, Subcommand};
use fs2::FileExt;
use serde_json::{json, Value};
use state::{Config, Range, Result};

const BUILD: &str = match option_env!("WFSIM_CLI_BUILD") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(name = "wfsim", version=BUILD, about = "Join WFSim volunteer computing")]
struct Args {
    #[arg(
        long,
        global = true,
        help = "Persistent device directory (also WFSIM_CLI_HOME)"
    )]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Join anonymously in this terminal; Ctrl+C returns unfinished work.
    Join {
        #[arg(long, hide = true)]
        worker: bool,
        #[arg(long, help = "Accept the displayed volunteer computing statement")]
        yes: bool,
        #[arg(long)]
        range: Option<Range>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Start in the background, writing client.log in the device directory.
    Start {
        #[arg(long)]
        yes: bool,
    },
    /// Stop accepting work and return unfinished leases.
    Stop,
    /// Pause computing while keeping the client running.
    Pause,
    /// Resume computing with the saved CPU range.
    Resume,
    /// Show status; --refresh also reads confirmed anonymous contribution.
    Status {
        #[arg(long)]
        refresh: bool,
    },
    /// Show or change the integer CPU range and device name.
    Config {
        #[arg(long)]
        range: Option<Range>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Claim this device through a browser on any computer or phone.
    Login,
    /// Print or install the local startup service.
    Service {
        #[command(subcommand)]
        command: Service,
    },
    /// Compute a local board fixture without accepting or submitting live work.
    EngineCheck { input: PathBuf },
}

#[derive(Subcommand)]
enum Service {
    Install,
    Remove,
}

fn main() {
    if let Err(e) = run() {
        if e.downcast_ref::<runner::EngineChanged>().is_some() {
            std::process::exit(supervisor::ENGINE_CHANGED);
        }
        eprintln!("WFSim: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let dir = args.data_dir.map(Ok).unwrap_or_else(state::data_dir)?;
    state::directory(&dir)?;
    let dir = fs::canonicalize(dir)?;
    let lock = state::private_options()
        .read(true)
        .create(true)
        .truncate(false)
        .open(dir.join("config.lock"))?;
    lock.lock_exclusive()?;
    let mut config = Config::load(&dir)?;
    match &args.command {
        Action::Join { range, name, .. } | Action::Config { range, name } => {
            if let Some(r) = range {
                config.range = r.clone();
            }
            if let Some(n) = name {
                let n = n.trim();
                if n.is_empty() || n.chars().count() > 40 || n.chars().any(char::is_control) {
                    return Err(
                        "device name must be 1–40 characters without control characters".into(),
                    );
                }
                config.name = n.into();
            }
        }
        Action::Pause => config.paused = true,
        Action::Resume => config.paused = false,
        _ => {}
    }
    match &args.command {
        Action::Join { yes, .. } | Action::Start { yes } => consent(&mut config, *yes)?,
        _ => {}
    }
    config.save(&dir)?;
    if matches!(
        args.command,
        Action::Join { .. }
            | Action::Start { .. }
            | Action::Login
            | Action::EngineCheck { .. }
            | Action::Status { refresh: true }
    ) {
        let origin = dir.join("origin");
        if origin.exists() {
            if fs::read_to_string(&origin)? != config.server {
                return Err("this device belongs to another server; restore config.json or use a separate --data-dir".into());
            }
        } else {
            let mut file = state::private_options().create_new(true).open(origin)?;
            file.write_all(config.server.as_bytes())?;
            file.sync_all()?;
        }
    }
    // Identity creation is serialized separately from the long-running process.
    let id = if matches!(
        args.command,
        Action::Status { .. } | Action::Stop | Action::Service { .. }
    ) {
        None
    } else {
        Some(state::identity(&dir)?)
    };
    FileExt::unlock(&lock)?;
    match args.command {
        Action::Join { worker, .. } => {
            let running = if worker {
                Some(instance(&dir)?)
            } else {
                let probe = instance(&dir)?;
                drop(probe);
                None
            };
            let quit = Arc::new(AtomicBool::new(false));
            let stop = Arc::clone(&quit);
            let handler = ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed));
            #[cfg(not(windows))]
            handler?;
            #[cfg(windows)]
            if let Err(error) = handler {
                if io::stdin().is_terminal() {
                    return Err(error.into());
                }
            }
            if !worker {
                return supervisor::run(&dir, quit);
            }
            println!(
                "WFSim CLI {} · device {} · CPU {}–{}%",
                BUILD,
                &id.as_ref().expect("identity")[..6],
                config.range.min,
                config.range.max
            );
            let result = runner::run(&dir, id.as_ref().expect("identity"), quit);
            drop(running);
            result?;
        }
        Action::Start { .. } => {
            let running = instance(&dir)?;
            drop(running);
            let log = state::private_options()
                .create(true)
                .append(true)
                .open(dir.join("client.log"))?;
            let mut cmd = Command::new(std::env::current_exe()?);
            cmd.arg("--data-dir")
                .arg(&dir)
                .arg("join")
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log);
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                // SAFETY: setsid is async-signal-safe and called only in the child.
                unsafe {
                    cmd.pre_exec(|| {
                        if libc::setsid() < 0 {
                            Err(io::Error::last_os_error())
                        } else {
                            Ok(())
                        }
                    });
                }
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x00000200 | 0x00000008);
            }
            let child = cmd.spawn()?;
            println!(
                "Started process {}. Status: wfsim status; log: {}",
                child.id(),
                dir.join("client.log").display()
            );
        }
        Action::Stop => {
            let status: Value = state::read(&dir.join("status.json"))?;
            if let Some(run) = status["run"].as_str() {
                state::write(&dir.join("stop.json"), &json!({"run":run}))?;
                println!(
                    "Stop requested; completed answers are kept and unfinished work is returned."
                );
            }
        }
        Action::Status { refresh } => {
            let mut status = state::read::<Value>(&dir.join("status.json"))
                .unwrap_or(json!({"state":"not started"}));
            if instance(&dir).is_ok() {
                status["state"] = json!("stopped");
            }
            status["config"] = serde_json::to_value(&config)?;
            status["directory"] = json!(dir);
            if refresh {
                let id = fs::read_to_string(dir.join("identity"))?;
                status["contribution"] = net::Net::new(&config.server)
                    .post("/api/board/points", &json!({"verifier":id}))?;
            }
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        Action::Config { .. } => println!("{}", serde_json::to_string_pretty(&config)?),
        Action::Pause => println!("Paused. The running client will return unfinished work."),
        Action::Resume => println!(
            "Resumed at {}–{}%. Start with wfsim join or wfsim start if stopped.",
            config.range.min, config.range.max
        ),
        Action::Login => login(&config, id.as_ref().expect("identity"))?,
        Action::Service { command } => service(&dir, &config, command)?,
        Action::EngineCheck { input } => {
            let task: Value = state::read(&input)?;
            let net = net::Net::new(&config.server);
            let release = net.get(&format!("{}/release.json", config.server))?.0;
            let engine = Arc::new(engine::Engine::load(
                &net,
                &dir,
                release["engine"].as_str().ok_or("missing engine")?,
            )?);
            let budget = Arc::new(budget::Budget::new(
                std::thread::available_parallelism()?.get().min(256),
            ));
            budget.set(100.0);
            let (tx, _rx) = std::sync::mpsc::channel();
            let result = if task["kind"] == "riven_gain" {
                compute::search(&engine, &budget, &task, &tx)?
            } else {
                compute::row(&engine, &budget, &task, &tx)?
            };
            println!(
                "{}",
                serde_json::to_string(&json!({"engine":engine.id,"result":result}))?
            );
        }
    }
    Ok(())
}

fn consent(config: &mut Config, yes: bool) -> Result<()> {
    if config
        .consent
        .as_ref()
        .is_some_and(|c| c.v == state::CONSENT_VERSION)
    {
        return Ok(());
    }
    println!(
        "{}\n\nCPU range: {}–{}%",
        state::CONSENT,
        config.range.min,
        config.range.max
    );
    if !yes {
        if !io::stdin().is_terminal() {
            return Err("read the statement, then rerun with --yes to participate".into());
        }
        print!("\nJoin volunteer computing? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err("participation was not enabled".into());
        }
    }
    config.consent = Some(state::Consent {
        v: state::CONSENT_VERSION,
        at: state::timestamp(),
    });
    Ok(())
}

fn instance(dir: &Path) -> Result<fs::File> {
    let file = state::private_options()
        .read(true)
        .create(true)
        .truncate(false)
        .open(dir.join("running.lock"))?;
    file.try_lock_exclusive()
        .map_err(|_| "a client already runs for this device; use wfsim status/pause/stop")?;
    Ok(file)
}

fn login(config: &Config, id: &str) -> Result<()> {
    let net = net::Net::new(&config.server);
    let start = net.post(
        "/api/account/devices/link/start",
        &json!({"verifier":id,"label":config.name}),
    )?;
    let token = start["device_code"]
        .as_str()
        .ok_or("device authorization is unavailable")?;
    println!(
        "Open {}\nCode: {}\nApprove this device to claim its unclaimed contribution.",
        start["verification_uri_complete"]
            .as_str()
            .ok_or("missing authorization URL")?,
        start["user_code"].as_str().unwrap_or("")
    );
    let expires = Instant::now() + Duration::from_secs(start["expires_in"].as_u64().unwrap_or(600));
    let interval = Duration::from_secs(start["interval"].as_u64().unwrap_or(5).clamp(5, 60));
    while Instant::now() < expires {
        std::thread::sleep(interval);
        match net.post(
            "/api/account/devices/link/poll",
            &json!({"device_code":token}),
        ) {
            Ok(r) if r["status"] == "claimed" => {
                println!(
                    "Device claimed. View your devices at {}/compute",
                    config.server
                );
                return Ok(());
            }
            Ok(r) if r["status"] == "expired" => break,
            Ok(r) if r["status"] == "pending" => {}
            Ok(r) => return Err(format!("authorization: {r}").into()),
            Err(e) => eprintln!("Authorization connection interrupted: {e}; retrying"),
        }
    }
    Err("authorization expired; run wfsim login again".into())
}

fn service(dir: &Path, config: &Config, action: Service) -> Result<()> {
    if matches!(action, Service::Install)
        && !config
            .consent
            .as_ref()
            .is_some_and(|c| c.v == state::CONSENT_VERSION)
    {
        return Err(
            "run wfsim join once and accept participation before installing a service".into(),
        );
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var_os("HOME").ok_or("HOME is unavailable")?;
        let root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".config"))
            .join("systemd/user");
        fs::create_dir_all(&root)?;
        let unit = root.join("wfsim-compute.service");
        let systemctl = |args: &[&str]| -> Result<()> {
            if !Command::new("systemctl")
                .arg("--user")
                .args(args)
                .status()?
                .success()
            {
                return Err(
                    "systemctl --user failed; check that your user service manager is running"
                        .into(),
                );
            }
            Ok(())
        };
        match action {
            Service::Install => {
                let quote = |p: &Path| -> Result<String> {
                    let s = p.to_str().ok_or("service path is not UTF-8")?;
                    if s.contains(['\n', '\r', '\0']) {
                        return Err("invalid service path".into());
                    }
                    Ok(format!(
                        "\"{}\"",
                        s.replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('%', "%%")
                            .replace('$', "$$")
                    ))
                };
                fs::write(&unit,format!("[Unit]\nDescription=WFSim volunteer computing\nAfter=network-online.target\n\n[Service]\nExecStart={} --data-dir {} join\nRestart=on-failure\nRestartSec=15\nTimeoutStopSec=60\n\n[Install]\nWantedBy=default.target\n",quote(&std::env::current_exe()?)?,quote(dir)?))?;
                systemctl(&["daemon-reload"])?;
                systemctl(&["enable", "--now", "wfsim-compute.service"])?;
                println!("User service installed. For operation after logout/reboot, enable user lingering with: loginctl enable-linger {}",std::env::var("USER").unwrap_or_default());
            }
            Service::Remove => {
                systemctl(&["disable", "--now", "wfsim-compute.service"])?;
                if unit.exists() {
                    fs::remove_file(unit)?;
                }
                systemctl(&["daemon-reload"])?;
                println!("Service removed; device identity and contribution are preserved.");
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (dir, config, action);
        Err("automatic startup installation currently supports Linux systemd user services; use wfsim start for background operation on this platform".into())
    }
}
