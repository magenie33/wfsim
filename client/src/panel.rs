// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::state::{self, Config, Range, Result};
use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde_json::Value;
use std::{
    fmt,
    fs::File,
    io::{self, IsTerminal, Stdout, Write},
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use unicode_width::UnicodeWidthChar;

const COMMANDS: &str = "/cpu 0-100  /pause  /resume  /quit";

pub fn supported() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && io::stderr().is_terminal()
        && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
}

pub struct Panel {
    output: Stdout,
    log: File,
    input: String,
    message: String,
    frame: Option<(u16, u16, Vec<String>)>,
}

impl Panel {
    pub fn open(dir: &Path) -> io::Result<Self> {
        let log = state::private_options()
            .create(true)
            .append(true)
            .open(dir.join("client.log"))?;
        terminal::enable_raw_mode()?;
        let mut panel = Self {
            output: io::stdout(),
            log,
            input: String::new(),
            message: String::new(),
            frame: None,
        };
        execute!(panel.output, EnterAlternateScreen)?;
        Ok(panel)
    }

    pub fn capture(&self, child: &mut Command) -> io::Result<()> {
        child
            .stdin(Stdio::null())
            .stdout(self.log.try_clone()?)
            .stderr(self.log.try_clone()?);
        Ok(())
    }

    pub fn notice(&mut self, message: fmt::Arguments<'_>) {
        self.message = message.to_string();
        let _ = writeln!(self.log, "{}", self.message);
    }

    pub fn tick(&mut self, dir: &Path, quit: &AtomicBool) -> Result<()> {
        for _ in 0..64 {
            if quit.load(Ordering::Relaxed) || !event::poll(Duration::ZERO)? {
                break;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    quit.store(true, Ordering::Relaxed);
                }
                KeyCode::Enter => {
                    let input = std::mem::take(&mut self.input);
                    if !input.trim().is_empty() {
                        self.message =
                            command(dir, quit, &input).unwrap_or_else(|error| error.to_string());
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
        }
        self.draw(dir, quit.load(Ordering::Relaxed))
    }

    fn draw(&mut self, dir: &Path, stopping: bool) -> Result<()> {
        let (width, height) = terminal::size()?;
        if width < 2 || height == 0 {
            return Ok(());
        }
        let status: Value = state::read(&dir.join("status.json")).unwrap_or(Value::Null);
        let config = Config::load(dir)?;
        let phase = if stopping {
            "Returning unfinished work"
        } else {
            status["state"].as_str().unwrap_or("Starting")
        };
        let percent = |key: &str| match status[key].as_f64() {
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
        lines.push(COMMANDS.into());
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

fn command(dir: &Path, quit: &AtomicBool, input: &str) -> Result<String> {
    let mut words = input.trim().trim_start_matches('/').split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some("cpu"), Some(range), None) => {
            let range: Range = range.parse()?;
            let message = format!("CPU range saved at {}-{}%", range.min, range.max);
            configure(dir, |config| config.range = range)?;
            Ok(message)
        }
        (Some("pause"), None, None) => {
            configure(dir, |config| config.paused = true)?;
            Ok("Paused".into())
        }
        (Some("resume"), None, None) => {
            configure(dir, |config| config.paused = false)?;
            Ok("Resumed".into())
        }
        (Some("quit"), None, None) => {
            quit.store(true, Ordering::Relaxed);
            Ok(String::new())
        }
        (Some("help"), None, None) => Ok(COMMANDS.into()),
        _ => Err("Try /cpu 0-100, /pause, /resume, or /quit.".into()),
    }
}

fn configure(dir: &Path, edit: impl FnOnce(&mut Config)) -> Result<()> {
    let _lock = state::lock_config(dir)?;
    let mut config = Config::load(dir)?;
    edit(&mut config);
    config.save(dir)
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
