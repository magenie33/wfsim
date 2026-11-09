// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::state::Result;

#[derive(Debug)]
pub struct ExpiredAppraisal;
impl std::fmt::Display for ExpiredAppraisal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no such appraisal")
    }
}
impl std::error::Error for ExpiredAppraisal {}
use serde_json::Value;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone)]
pub struct Net {
    pub base: String,
    agent: ureq::Agent,
    clock: Arc<Mutex<Option<(Instant, SystemTime)>>>,
}

impl Net {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').into(),
            clock: Arc::new(Mutex::new(None)),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(45))
                .redirects(0)
                .user_agent(&format!("WFSim-CLI/{}", crate::BUILD))
                .build(),
        }
    }
    pub fn get(&self, url: &str) -> Result<(Value, Option<SystemTime>)> {
        let r = self.agent.get(url).call()?;
        let date = r
            .header("Date")
            .and_then(|s| httpdate::parse_http_date(s).ok());
        Ok((
            serde_json::from_slice(&read_bytes(r, 4 * 1024 * 1024)?)?,
            date,
        ))
    }
    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let text = serde_json::to_string(body)?;
        if text.len() > 262_144 {
            return Err("request exceeds the compute protocol's body limit".into());
        }
        let sent = self
            .agent
            .post(&format!("{}{path}", self.base))
            .set("Content-Type", "application/json")
            .send_string(&text);
        let r = match sent {
            Ok(r) => r,
            Err(ureq::Error::Status(404, response))
                if path.starts_with("/api/appraise/") && path.ends_with("/result") =>
            {
                let error: Value = serde_json::from_slice(&read_bytes(response, 4 * 1024 * 1024)?)?;
                if error["error"] == "no such appraisal" {
                    return Err(ExpiredAppraisal.into());
                }
                return Err(format!("HTTP 404: {error}").into());
            }
            Err(error) => return Err(error.into()),
        };
        if let Some(date) = r
            .header("Date")
            .and_then(|s| httpdate::parse_http_date(s).ok())
        {
            *self.clock.lock().expect("server clock") = Some((Instant::now(), date));
        }
        Ok(serde_json::from_slice(&read_bytes(r, 4 * 1024 * 1024)?)?)
    }
    pub fn until(&self, iso: &str) -> Option<Duration> {
        let at = time::OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339)
            .ok()?;
        let target =
            SystemTime::UNIX_EPOCH + Duration::from_secs(at.unix_timestamp().max(0) as u64);
        let now = self
            .clock
            .lock()
            .ok()?
            .map(|(seen, date)| date + seen.elapsed())
            .unwrap_or_else(SystemTime::now);
        Some(target.duration_since(now).unwrap_or(Duration::ZERO))
    }
    pub fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>> {
        read_bytes(self.agent.get(url).call()?, limit)
    }
}

fn read_bytes(r: ureq::Response, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    r.into_reader().take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("response exceeds its size limit".into());
    }
    Ok(bytes)
}
