// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// Keep the simulator's original JSON numbers, including integral work above 2^53.
#[derive(Serialize, Deserialize)]
pub enum FinalScore {
    Measured([Value; 7]),
    Refused(Value),
}

const FIELDS: [(&str, &str); 7] = [
    ("kill_progress", "score"),
    ("kill_progress_se", "score_se"),
    ("kills", "kills"),
    ("kills_min", "kills_min"),
    ("kills_max", "kills_max"),
    ("dps", "dps"),
    ("work", "work"),
];

impl FinalScore {
    pub fn simulate(replay: &Value, progress: &mut impl FnMut(u32, u32)) -> Self {
        let sim = crate::simulate::simulate_json_reporting(replay, progress);
        if sim["ok"] == false {
            return Self::Refused(sim["error"].clone());
        }
        Self::Measured(FIELDS.map(|(_, from)| sim[from].clone()))
    }

    fn apply(self, row: &mut Value) {
        match self {
            Self::Refused(reason) => {
                row["simulator_error"] = reason;
                row["kill_progress"] = Value::Null;
            }
            Self::Measured(values) => {
                for ((to, _), value) in FIELDS.into_iter().zip(values) {
                    row[to] = value;
                }
            }
        }
    }
}

/// The leader owns the rows in screen order. A worker only needs one whole replay.
pub struct FinalRound {
    pub(super) report: Value,
    pub(super) rows: Vec<Value>,
}

impl FinalRound {
    pub fn replays(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.rows.iter().map(|r| &r["replay"])
    }

    pub fn measured(mut self, scores: Vec<FinalScore>) -> Value {
        if scores.len() != self.rows.len() {
            return crate::err_json("incomplete finalist measurements");
        }
        for (row, score) in self.rows.iter_mut().zip(scores) {
            score.apply(row);
        }
        let work = self.rows.iter().fold(0u64, |a, r| {
            a.saturating_add(r["work"].as_u64().unwrap_or(0))
        });
        let at = |r: &Value, k: &str| r[k].as_f64().unwrap_or(f64::NEG_INFINITY);
        self.rows.sort_by(|a, b| {
            at(b, "kill_progress")
                .total_cmp(&at(a, "kill_progress"))
                .then(at(b, "dps").total_cmp(&at(a, "dps")))
        });
        for (i, row) in self.rows.iter_mut().enumerate() {
            row["rank"] = json!(i + 1);
        }
        self.report(work)
    }

    pub(super) fn report(mut self, work: u64) -> Value {
        let mut by_start = std::collections::BTreeMap::new();
        for row in &self.rows {
            for lane in row["from_starts"].as_array().into_iter().flatten() {
                if let Some(i) = lane["start"].as_u64() {
                    by_start.entry(i).or_insert(row);
                }
            }
        }
        self.report["start_results"] = Value::Array(
            by_start
                .into_iter()
                .map(|(start, row)| json!({"start":start,"result":row}))
                .collect(),
        );
        self.rows
            .truncate(self.report["finalists"].as_u64().expect("plan finalists") as usize);
        self.report["results"] = Value::Array(self.rows);
        self.report["final_work"] = json!(work);
        self.report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ties_keep_screen_order_and_unlisted_contenders_still_count() {
        let round = || FinalRound {
            report: json!({"finalists":1}),
            rows: vec![
                json!({"mod":"a","from_starts":[{"start":0}]}),
                json!({"mod":"b","from_starts":[{"start":1}]}),
                json!({"mod":"c"}),
            ],
        };
        let score = |work| {
            FinalScore::Measured([
                json!(1.5),
                json!(0.1),
                json!(3),
                json!(2),
                json!(4),
                json!(10),
                json!(work),
            ])
        };
        let big = (1_u64 << 53) + 1;
        let result = round().measured(vec![
            score(big),
            score(4),
            FinalScore::Refused(json!("not legal")),
        ]);
        assert_eq!(result["results"][0]["mod"], "a");
        assert_eq!(result["start_results"][1]["result"]["rank"], 2);
        assert_eq!(result["final_work"], big + 4);
        assert_eq!(round().measured(vec![])["ok"], false);
    }
}
