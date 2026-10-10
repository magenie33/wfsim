//! The cookies `wfsim.app` sets, kept by the shell.
//!
//! THE SHELL HOLDS THEM, NOT THE WEBVIEW. The page lives at `wfsim.localhost`
//! and the cookies belong to `wfsim.app`, so a webview that kept them would
//! keep them under the wrong name and never send them back. Every request to
//! the site already passes through `protocol::proxy`, which is the one place
//! that can attach them and the one place that sees them set.
//!
//! The session cookie is a bearer credential: it sits in the app's own data
//! directory, where a browser would keep its own, and nowhere else.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct Kept {
    v: String,
    /// Unix seconds; a cookie past it is not sent and not kept.
    until: u64,
}

pub struct Jar {
    path: PathBuf,
    cookies: Mutex<BTreeMap<String, Kept>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Jar {
    pub fn open(path: PathBuf) -> Self {
        let cookies = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { path, cookies: Mutex::new(cookies) }
    }

    /// The `Cookie` header for the next request, or `None` with nothing to send.
    pub fn header(&self) -> Option<String> {
        let t = now();
        let jar = self.cookies.lock().expect("cookie jar");
        let line = jar
            .iter()
            .filter(|(_, k)| k.until > t)
            .map(|(n, k)| format!("{n}={}", k.v))
            .collect::<Vec<_>>()
            .join("; ");
        (!line.is_empty()).then_some(line)
    }

    /// Take what a response set, and write the jar if anything moved.
    pub fn take<'a>(&self, set_cookies: impl IntoIterator<Item = &'a str>) {
        let t = now();
        let mut jar = self.cookies.lock().expect("cookie jar");
        let before = jar.clone();
        for line in set_cookies {
            if let Some((name, kept)) = parse(line, t) {
                match kept {
                    Some(k) => jar.insert(name, k),
                    None => jar.remove(&name),
                };
            }
        }
        jar.retain(|_, k| k.until > t);
        if *jar != before {
            if let Ok(body) = serde_json::to_vec(&*jar) {
                let tmp = self.path.with_extension("part");
                if std::fs::write(&tmp, body).is_ok() {
                    let _ = std::fs::rename(&tmp, &self.path);
                }
            }
        }
    }
}

/// One `Set-Cookie` line: the name, and what to keep — `None` to forget it.
///
/// Every cookie the site sets carries `Max-Age` (worker/accounts.js
/// `setCookie`), and a `Max-Age` of 0 is how it signs a reader out. One
/// without it lasts as long as a browser session would: until the app closes,
/// which this jar approximates as a day.
fn parse(line: &str, t: u64) -> Option<(String, Option<Kept>)> {
    let mut parts = line.split(';');
    let (name, v) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut until = t + 24 * 3600;
    for attr in parts {
        if let Some((k, val)) = attr.split_once('=') {
            if k.trim().eq_ignore_ascii_case("max-age") {
                let secs: i64 = val.trim().parse().ok()?;
                if secs <= 0 {
                    return Some((name.to_string(), None));
                }
                until = t + secs as u64;
            }
        }
    }
    let v = v.trim();
    Some((name.to_string(), (!v.is_empty()).then(|| Kept { v: v.to_string(), until })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jar() -> Jar {
        let path = std::env::temp_dir().join(format!("wfsim-jar-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Jar::open(path)
    }

    #[test]
    fn a_session_is_kept_sent_and_forgotten_on_sign_out() {
        let j = jar();
        assert_eq!(j.header(), None);
        j.take(["wfsim_session=abc; Path=/; Max-Age=2592000; HttpOnly; Secure; SameSite=Lax"]);
        j.take(["wfsim_oauth=x.y; Path=/api/auth; Max-Age=600; HttpOnly; Secure; SameSite=Lax"]);
        assert_eq!(j.header().as_deref(), Some("wfsim_oauth=x.y; wfsim_session=abc"));
        // …AND IT SURVIVES A RESTART, which is the point of keeping it on disk.
        let again = Jar::open(j.path.clone());
        assert_eq!(again.header().as_deref(), Some("wfsim_oauth=x.y; wfsim_session=abc"));
        j.take(["wfsim_session=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax"]);
        assert_eq!(j.header().as_deref(), Some("wfsim_oauth=x.y"));
        let _ = std::fs::remove_file(&j.path);
    }

    #[test]
    fn a_line_that_is_not_a_cookie_changes_nothing() {
        let t = 1000;
        for bad in ["", "=v", "noequals", "a=b; Max-Age=soon"] {
            assert_eq!(parse(bad, t), None, "{bad:?}");
        }
        assert_eq!(parse("a=b; max-age=10", t), Some(("a".into(), Some(Kept { v: "b".into(), until: 1010 }))));
    }
}
