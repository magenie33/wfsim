//! WHAT A RUN IS JUDGED BY — declared once, for every surface that ranks.
//!
//! A metric is a term of the SCENARIO, not of the code that reads one. Two are
//! shipped and there will be more, so the shape that matters is that adding one
//! is an entry in [`ALL`] and nothing else: the scorer, the board, the headline
//! number, the picker's gain scan and the Measure control all resolve an id
//! against this table rather than asking "is it dps".
//!
//! THAT QUESTION IS THE FAILURE MODE. `metric === "dps" ? … : KPM` reads a
//! third metric as kills per minute — silently, in the units of a different
//! question — and it was written eight times across the engine, the scorer and
//! the page. [`get`] returns `None` instead, so an unknown id is a refusal
//! somebody has to answer rather than a number nobody can trust.

use serde::Serialize;

/// One way of judging a run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MetricDef {
    pub id: &'static str,
    /// The field of a simulate response the value is read from.
    pub field: &'static str,
    /// Is that field a TOTAL over the engagement, to be turned into a rate?
    /// `score` is kill progress over the whole fight; `dps` is already a rate.
    pub per_minute: bool,
    /// The unit, beside the number. Translated on the page.
    pub label: &'static str,
    /// What it means, for the control that offers it.
    pub hint: &'static str,
}

/// EVERY METRIC, and the order the Measure control offers them in.
///
/// KPM IS FIRST BECAUSE IT IS WHAT A BUILD IS FOR. DPS is the other honest
/// answer and the only one left when a target cannot be killed at all.
pub const ALL: &[MetricDef] = &[
    MetricDef {
        id: "kpm",
        field: "score",
        per_minute: true,
        label: "KPM",
        hint: "kills per minute",
    },
    MetricDef {
        id: "dps",
        field: "dps",
        per_minute: false,
        label: "DPS",
        hint: "damage per second",
    },
];

/// The metric with this id, or `None` — which every caller must answer for.
pub fn get(id: &str) -> Option<&'static MetricDef> {
    ALL.iter().find(|m| m.id == id)
}

/// The default when a scenario names none.
pub const DEFAULT: &str = "kpm";

impl MetricDef {
    /// The run's number IN THIS METRIC, given the raw field and the engagement.
    ///
    /// A rate over an engagement of zero length is zero rather than an
    /// infinity: no time passed, so nothing happened per minute of it.
    pub fn of(&self, raw: f64, duration: f64) -> f64 {
        if !self.per_minute {
            return raw;
        }
        if duration > 0.0 {
            raw * 60.0 / duration
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_metric_is_looked_up_and_an_unknown_one_is_refused() {
        assert_eq!(get("kpm").map(|m| m.field), Some("score"));
        assert_eq!(get("dps").map(|m| m.field), Some("dps"));
        // THE POINT OF THE TABLE. A caller that asked "is it dps" would read
        // this as kills per minute and publish it in the wrong units.
        assert!(get("hits_per_magazine").is_none());
        assert!(get("").is_none());
    }

    #[test]
    fn a_total_becomes_a_rate_and_a_rate_is_left_alone() {
        let kpm = get("kpm").unwrap();
        // 33 kills over 180 s is 11 a minute.
        assert!((kpm.of(33.0, 180.0) - 11.0).abs() < 1e-12);
        // NO TIME, NO RATE — never an infinity in a published number.
        assert_eq!(kpm.of(33.0, 0.0), 0.0);
        let dps = get("dps").unwrap();
        assert_eq!(dps.of(41_774.0, 180.0), 41_774.0);
    }

    #[test]
    fn every_metric_is_distinct_and_states_a_unit() {
        for (i, m) in ALL.iter().enumerate() {
            assert!(!m.id.is_empty() && !m.label.is_empty() && !m.hint.is_empty(), "{m:?}");
            assert!(ALL[..i].iter().all(|o| o.id != m.id), "duplicate id {}", m.id);
        }
        assert_eq!(get(DEFAULT).map(|m| m.id), Some(DEFAULT));
    }
}
