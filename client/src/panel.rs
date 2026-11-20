// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::state::{self, Config, Result, CONSENT_VERSION};
use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use fs2::FileExt;
use serde_json::Value;
use std::{
    fs::File,
    io::{self, IsTerminal, Stdout, Write},
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use unicode_width::UnicodeWidthChar;

pub fn supported() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && io::stderr().is_terminal()
        && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
}

pub fn run(dir: &Path) -> Result<()> {
    let closing = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&closing);
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;

    let mut panel = Panel::open()?;
    while !closing.load(Ordering::Relaxed) {
        panel.draw(dir)?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if !panel.key(dir, key) {
                    break;
                }
            }
        }
    }
    Ok(())
}

struct Panel {
    output: Stdout,
    input: String,
    message: String,
    frame: Option<(u16, u16, Vec<String>)>,
}

impl Panel {
    fn open() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut panel = Self {
            output: io::stdout(),
            input: String::new(),
            message: String::new(),
            frame: None,
        };
        execute!(panel.output, EnterAlternateScreen)?;
        Ok(panel)
    }

    fn key(&mut self, dir: &Path, key: event::KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return true;
        }
        match key.code {
            KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return false;
            }
            KeyCode::Enter => {
                let input = std::mem::take(&mut self.input);
                if !input.trim().is_empty() {
                    match self.command(dir, &input) {
                        Ok(Some(message)) => self.message = message,
                        Ok(None) => return false,
                        Err(error) => self.message = error.to_string(),
                    }
                }
            }
            KeyCode::Esc => self.input.clear(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c)
                if self.input.len() < 64
                    && (c.is_ascii_graphic() || c == ' ')
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.input.push(c);
            }
            _ => {}
        }
        true
    }

    fn command(&mut self, dir: &Path, input: &str) -> Result<Option<String>> {
        let mut words = input.trim().trim_start_matches('/').split_whitespace();
        let (args, success): (&[&str], &str) = match (words.next(), words.next(), words.next()) {
            (Some("start"), None, None) => (&["start"], "Starting background computing."),
            (Some("stop"), None, None) if !computing(dir) => {
                return Ok(Some("Computing is stopped.".into()));
            }
            (Some("stop"), None, None) => (&["stop"], "Returning unfinished work and stopping."),
            (Some("login"), None, None) => (&["login"], "Device linked."),
            (Some("cpu"), Some(range), None) => (&["config", "--range", range], "CPU range saved."),
            (Some("pause"), None, None) => (&["pause"], "Paused."),
            (Some("resume"), None, None) => (&["resume"], "Resumed."),
            (Some("quit"), None, None) => return Ok(None),
            (Some("help"), None, None) => {
                return Ok(Some("Type any command below, then press Enter.".into()));
            }
            _ => return Err("Unknown command. Type /help to see how to use the panel.".into()),
        };

        let needs_terminal = match args[0] {
            "login" => true,
            "start" => Config::load(dir)?
                .consent
                .is_none_or(|consent| consent.v != CONSENT_VERSION),
            _ => false,
        };
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--data-dir").arg(dir).args(args);
        if needs_terminal {
            terminal::disable_raw_mode()?;
            execute!(self.output, Clear(ClearType::All), MoveTo(0, 0))?;
            let status = command.status();
            terminal::enable_raw_mode()?;
            self.frame = None;
            if !status?.success() {
                return Err("Command did not complete.".into());
            }
        } else {
            let result = command.output()?;
            if !result.status.success() {
                let error = String::from_utf8_lossy(&result.stderr);
                let message = error.lines().find(|line| !line.trim().is_empty());
                return Err(message.unwrap_or("Command failed.").to_owned().into());
            }
        }
        Ok(Some(success.into()))
    }

    fn draw(&mut self, dir: &Path) -> Result<()> {
        let (width, height) = terminal::size()?;
        if width < 2 || height == 0 {
            return Ok(());
        }
        let status: Value = state::read(&dir.join("status.json")).unwrap_or(Value::Null);
        let config = Config::load(dir)?;
        let active = computing(dir);
        let phase = if active {
            status["state"].as_str().unwrap_or("starting")
        } else {
            "stopped"
        };
        let percent = |key: &str| match status[key].as_f64().filter(|_| active) {
            Some(value) => format!("{value:.0}%"),
            None => String::from("--"),
        };
        let mut lines = vec![
            format!("WFSim | {phase}"),
            format!(
                "CPU {}-{}% | Target {} | In use {}",
                config.range.min,
                config.range.max,
                percent("target_percent"),
                percent("cpu_percent")
            ),
            format!(
                "Completed {} | Pending submission {}",
                status["completed"].as_u64().unwrap_or(0),
                status["pending"].as_u64().unwrap_or(0)
            ),
        ];
        if !self.message.is_empty() {
            lines.push(self.message.clone());
        }
        lines.push("/start  /stop  /login  /cpu 0-100".into());
        lines.push("/pause  /resume  /quit closes this panel".into());
        lines.truncate(usize::from(height - 1));
        let input_start = self
            .input
            .len()
            .saturating_sub(usize::from(width.saturating_sub(3)));
        lines.push(format!("> {}", &self.input[input_start..]));
        let lines: Vec<_> = lines.iter().map(|line| fit(line, width - 1)).collect();
        let frame = (width, height, lines);
        if self.frame.as_ref() == Some(&frame) {
            return Ok(());
        }
        queue!(self.output, Clear(ClearType::All))?;
        for (row, line) in frame.2.iter().enumerate() {
            queue!(self.output, MoveTo(0, row as u16), Print(line))?;
        }
        self.output.flush()?;
        self.frame = Some(frame);
        Ok(())
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        let _ = execute!(self.output, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn computing(dir: &Path) -> bool {
    let Ok(lock) = File::open(dir.join("supervisor.lock")) else {
        return false;
    };
    FileExt::try_lock_shared(&lock)
        .is_err_and(|error| error.raw_os_error() == fs2::lock_contended_error().raw_os_error())
}

fn fit(line: &str, width: u16) -> String {
    let mut remaining = usize::from(width);
    let mut clipped = String::new();
    for c in line.chars().filter(|c| !c.is_control()) {
        let Some(left) = remaining.checked_sub(c.width().unwrap_or(0)) else {
            break;
        };
        remaining = left;
        clipped.push(c);
    }
    clipped
}
