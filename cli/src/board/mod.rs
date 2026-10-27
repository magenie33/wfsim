// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE SCORER — turns submitted builds into a board.
//!
//! Reads the library as a JSON array on stdin, reads the open generation's
//! FACTS as a file, and writes one file per weapon plus a cross-weapon index.
//! Fetching either and committing the result are the workflow's job
//! (`.github/workflows/scores.yml`), and neither needs the engine. What needs
//! the engine is the only thing here — running each build under the benchmark
//! and reading the number off.
//!
//! WHY A SUBMISSION CARRIES NO SCORE: nobody's number is trusted because
//! nobody's number is asked for. A row's score is produced HERE under the
//! benchmark's own pinned seed, so anyone with the repo reproduces any row
//! exactly.
//!
//! IT NEVER SPEAKS TO THE DATABASE. Files in, files out; the network lives in
//! `scripts/ship_facts.sh` and `scripts/fetch_facts.sh`, which a stub `curl`
//! can drive — which is what makes every hop of the chain testable.
//!
//!   cat library.json | wfsim-board standard_single_target site/board //!     --facts-in facts-known.ndjson --facts facts.ndjson

/// A LINE OF A RULER'S LOG: to stderr, or into the buffer its thread holds when
/// `--each-ruler` walks several rulers at once and prints each log whole.
macro_rules! say {
    ($($t:tt)*) => { $crate::board::said(format!($($t)*)) };
}

thread_local! {
    static SINK: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn said(line: String) {
    SINK.with(|s| match s.borrow_mut().as_mut() {
        Some(log) => {
            log.push_str(&line);
            log.push('\n');
        }
        None => print_log(format!("{line}\n").as_bytes()),
    });
}

/// A runner's stderr can be a nonblocking pipe. Backpressure must retain the
/// unwritten suffix rather than panic halfway through a reconciliation.
fn write_log(out: &mut impl std::io::Write, mut bytes: &[u8]) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    while !bytes.is_empty() {
        match out.write(bytes) {
            Ok(0) => return Err(Error::from(ErrorKind::WriteZero)),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn print_log(bytes: &[u8]) {
    write_log(&mut std::io::stderr().lock(), bytes).expect("board log");
}

/// …AND WHAT `f` SAID, held rather than printed.
pub(crate) fn captured(f: impl FnOnce()) -> String {
    SINK.with(|s| *s.borrow_mut() = Some(String::new()));
    f();
    SINK.with(|s| s.borrow_mut().take()).unwrap_or_default()
}

mod assemble;
mod entry;
mod facts;
mod measure;
mod publish;
pub use publish::CLIENT_MEASURED;
mod queue;
mod run;
mod state;

pub use run::run;

#[cfg(test)]
mod log_tests {
    use std::collections::VecDeque;
    use std::io::{self, ErrorKind, Write};

    struct Pipe {
        writes: VecDeque<io::Result<usize>>,
        bytes: Vec<u8>,
    }

    impl Write for Pipe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let n = self.writes.pop_front().unwrap_or(Ok(bytes.len()))?;
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn pipe_backpressure_preserves_every_byte() {
        let mut pipe = Pipe {
            writes: [Ok(3), Err(ErrorKind::WouldBlock.into()), Ok(2),
                Err(ErrorKind::Interrupted.into()), Err(ErrorKind::WouldBlock.into())].into(),
            bytes: Vec::new(),
        };
        super::write_log(&mut pipe, b"refused build\nnext ruler\n").unwrap();
        assert_eq!(pipe.bytes, b"refused build\nnext ruler\n");
    }

    #[test]
    fn a_closed_log_is_an_error() {
        for result in [Ok(0), Err(ErrorKind::BrokenPipe.into())] {
            let mut pipe = Pipe { writes: [result].into(), bytes: Vec::new() };
            assert!(super::write_log(&mut pipe, b"refused build\n").is_err());
        }
    }
}
