// SPDX-License-Identifier: AGPL-3.0-or-later
//! PART VALUE ANALYSIS: what each chosen part of a build multiplies the result
//! by, as exact Shapley values over ln(metric). `docs/SHAPLEY.md`.
//!
//! This endpoint is arithmetic only. The page measures every subset through
//! `/api/simulate` — the simulator's own fight, one seed for all of them — and
//! hands the per-run series here, so nothing in this file can disagree with a
//! number the simulator prints.

use serde_json::{json, Value};

use crate::request::err_json;

/// THE MOST PARTICIPANTS ONE ANALYSIS TAKES. Every subset is simulated, so the
/// cost is 2^k fights; past this the page is asked for tens of thousands of
/// them. Served at `/api/meta.shapley_max_participants` so the page refuses
/// before it spends anything.
pub const MAX_PARTICIPANTS: usize = 14;

/// One statistic: a linear combination of ln(mean of subset S), with the
/// uncertainty of that combination over the paired runs.
///
/// DELTA METHOD, PAIRED. ln(x̄_S) moves by (x̄_S − μ_S)/μ_S to first order, so
/// Σ c_S ln x̄_S has per-run residual z_r = Σ c_S x_{S,r}/x̄_S and its standard
/// error is sd(z)/√n. Every subset ran on the same dice, so a part that scales
/// the fight proportionally cancels run by run and its band is exactly zero —
/// the same property `gainOver` states for one comparison.
fn stat(coef: &[f64], ln_mean: &[f64], ratio: &[Vec<f64>], runs: usize) -> Value {
    let log: f64 = coef.iter().zip(ln_mean).map(|(c, l)| c * l).sum();
    if runs < 2 {
        return json!({ "log": log, "se": Value::Null });
    }
    let mut z = vec![0.0; runs];
    for (s, c) in coef.iter().enumerate() {
        if *c == 0.0 {
            continue;
        }
        for (r, zr) in z.iter_mut().enumerate() {
            *zr += c * ratio[s][r];
        }
    }
    let mean = z.iter().sum::<f64>() / runs as f64;
    let ss: f64 = z.iter().map(|x| (x - mean) * (x - mean)).sum();
    let se = (ss / (runs - 1) as f64 / runs as f64).sqrt();
    // FLOATING-POINT RESIDUE IS NOT A BAND: a proportional part leaves z_r at
    // the last bits of a double, which would print as "±0.0%" and mean nothing.
    let scale: f64 = coef.iter().map(|c| c.abs()).sum::<f64>().max(1.0);
    json!({ "log": log, "se": if se < 1e-9 * scale { 0.0 } else { se } })
}

fn factorials(n: usize) -> Vec<f64> {
    let mut f = vec![1.0; n + 1];
    for i in 1..=n {
        f[i] = f[i - 1] * i as f64;
    }
    f
}

/// `POST /api/shapley`: `{ participants: [id; k], values: [[run; n]; 2^k] }`,
/// where `values[mask]` is the per-run series of the subset whose bit i is
/// participant i. Every subset carries the same n runs, drawn on one seed.
pub fn shapley_json(v: &Value) -> Value {
    let ids: Vec<String> = v
        .get("participants")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    let k = ids.len();
    if k == 0 {
        return err_json("no participants — choose at least one part to analyse");
    }
    if k > MAX_PARTICIPANTS {
        return err_json(format!(
            "{k} participants is 2^{k} fights; at most {MAX_PARTICIPANTS} are analysed exactly"
        ));
    }
    let Some(values) = v.get("values").and_then(Value::as_array) else {
        return err_json("values: one per-run series per subset is required");
    };
    if values.len() != 1 << k {
        return err_json(format!(
            "values: {} subsets for {k} participants, which needs {}",
            values.len(),
            1usize << k
        ));
    }
    let series: Vec<Vec<f64>> = values
        .iter()
        .map(|s| {
            s.as_array()
                .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect())
                .unwrap_or_default()
        })
        .collect();
    let runs = series[0].len();
    if runs == 0 || series.iter().any(|s| s.len() != runs) {
        return err_json("values: every subset must carry the same number of runs, one at least");
    }
    let mut ln_mean = Vec::with_capacity(series.len());
    let mut ratio = Vec::with_capacity(series.len());
    for (mask, s) in series.iter().enumerate() {
        let mean = s.iter().sum::<f64>() / runs as f64;
        // ln IS UNDEFINED AT ZERO, and a part that takes the result to nothing
        // has no multiplier to report. Named, so the reader can drop it.
        if !(mean.is_finite() && mean > 0.0) {
            let with: Vec<&str> =
                (0..k).filter(|i| mask >> i & 1 == 1).map(|i| ids[i].as_str()).collect();
            return err_json(format!(
                "the build with only [{}] of the chosen parts measured {mean} — a multiplier needs every subset above zero",
                with.join(", ")
            ));
        }
        ln_mean.push(mean.ln());
        ratio.push(s.iter().map(|x| x / mean).collect::<Vec<f64>>());
    }

    let n_sub = 1usize << k;
    let full = n_sub - 1;
    let fact = factorials(k);
    let one = |pairs: &[(usize, f64)]| {
        let mut c = vec![0.0; n_sub];
        for &(s, w) in pairs {
            c[s] += w;
        }
        stat(&c, &ln_mean, &ratio, runs)
    };

    let mut parts = Vec::with_capacity(k);
    for (i, id) in ids.iter().enumerate() {
        let bit = 1 << i;
        // φ_i = Σ_{S ∌ i} |S|!(k−|S|−1)!/k! · [v(S∪i) − v(S)]
        let mut c = vec![0.0; n_sub];
        for s in (0..n_sub).filter(|s| s & bit == 0) {
            let size = s.count_ones() as usize;
            let w = fact[size] * fact[k - size - 1] / fact[k];
            c[s | bit] += w;
            c[s] -= w;
        }
        parts.push(json!({
            "id": id,
            "shapley": stat(&c, &ln_mean, &ratio, runs),
            "leave_one_out": one(&[(full, 1.0), (full & !bit, -1.0)]),
            "alone": one(&[(bit, 1.0), (0, -1.0)]),
        }));
    }

    // THE SHAPLEY INTERACTION INDEX, in log space: above zero the pair is worth
    // more together than apart (synergy), below zero one dilutes the other.
    // I_ij = Σ_{S ⊆ N∖{i,j}} |S|!(k−|S|−2)!/(k−1)! · [v(S∪ij) − v(S∪i) − v(S∪j) + v(S)]
    let mut matrix = vec![vec![Value::Null; k]; k];
    let pairs = (0..k).flat_map(|i| ((i + 1)..k).map(move |j| (i, j)));
    for (i, j) in pairs {
        let (bi, bj) = (1 << i, 1 << j);
        let mut c = vec![0.0; n_sub];
        for s in (0..n_sub).filter(|s| s & (bi | bj) == 0) {
            let size = s.count_ones() as usize;
            let w = fact[size] * fact[k - size - 2] / fact[k - 1];
            c[s | bi | bj] += w;
            c[s | bi] -= w;
            c[s | bj] -= w;
            c[s] += w;
        }
        let x = stat(&c, &ln_mean, &ratio, runs);
        matrix[i][j] = x.clone();
        matrix[j][i] = x;
    }

    json!({
        "ok": true,
        "runs": runs,
        "subsets": n_sub,
        "full": ln_mean[full].exp(),
        "empty": ln_mean[0].exp(),
        "total": one(&[(full, 1.0), (0, -1.0)]),
        "participants": parts,
        "interactions": matrix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulate_json;

    fn logs(r: &Value, key: &str) -> Vec<f64> {
        r["participants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p[key]["log"].as_f64().unwrap())
            .collect()
    }

    /// A table from a value function of the mask, three runs that differ.
    fn table(k: usize, f: impl Fn(usize) -> f64) -> Value {
        let values: Vec<Vec<f64>> =
            (0..1usize << k).map(|m| vec![f(m) * 0.9, f(m), f(m) * 1.1]).collect();
        let ids: Vec<String> = (0..k).map(|i| format!("p{i}")).collect();
        json!({ "participants": ids, "values": values })
    }

    /// EFFICIENCY: the multipliers multiply out to full over empty, whatever
    /// the interactions are.
    #[test]
    fn the_multipliers_multiply_to_full_over_empty() {
        let f = |m: usize| {
            let b = |i: usize| (m >> i & 1) as f64;
            // a synergy, a dilution and a part worth less alone than nothing
            3.0 + 2.0 * b(0) + 2.0 * b(1) + 5.0 * b(0) * b(1) - 0.5 * b(2) + 4.0 * b(2) * b(3)
        };
        let r = shapley_json(&table(4, f));
        assert_eq!(r["ok"], true, "{r}");
        let sum: f64 = logs(&r, "shapley").iter().sum();
        let want = (f(15) / f(0)).ln();
        assert!((sum - want).abs() < 1e-12, "Σφ {sum} vs ln(full/empty) {want}");
        assert!((r["total"]["log"].as_f64().unwrap() - want).abs() < 1e-12);
    }

    /// Two parts on independent multipliers: each one's value is its effect
    /// alone, the pair does not interact, and a proportional table has no band.
    #[test]
    fn independent_multipliers_are_worth_what_they_are_alone() {
        let r = shapley_json(&table(2, |m| {
            (if m & 1 == 1 { 1.5 } else { 1.0 }) * (if m & 2 == 2 { 1.2 } else { 1.0 })
        }));
        let (phi, alone) = (logs(&r, "shapley"), logs(&r, "alone"));
        assert!((phi[0] - 1.5f64.ln()).abs() < 1e-12 && (phi[1] - 1.2f64.ln()).abs() < 1e-12);
        assert!((phi[0] - alone[0]).abs() < 1e-12 && (phi[1] - alone[1]).abs() < 1e-12);
        assert!(r["interactions"][0][1]["log"].as_f64().unwrap().abs() < 1e-12);
        assert_eq!(r["participants"][0]["shapley"]["se"].as_f64(), Some(0.0));
    }

    #[test]
    fn a_subset_at_zero_is_refused_by_name() {
        let r = shapley_json(&table(2, |m| if m == 2 { 0.0 } else { 1.0 }));
        let e = r["error"].as_str().unwrap_or("");
        assert!(e.contains("[p1]"), "{r}");
    }

    #[test]
    fn too_many_participants_are_refused_before_the_table_is_read() {
        let ids: Vec<String> = (0..=MAX_PARTICIPANTS).map(|i| format!("p{i}")).collect();
        let r = shapley_json(&json!({ "participants": ids, "values": [] }));
        assert!(r["error"].as_str().is_some_and(|e| e.contains("exactly")), "{r}");
    }

    /// THE WHOLE PATH, on the simulator: every subset measured through
    /// `/api/simulate` on one seed, the way the page does it.
    fn measure(mods: &[&str], metric: &str) -> Value {
        let k = mods.len();
        let values: Vec<Value> = (0..1usize << k)
            .map(|m| {
                let on: Vec<&str> =
                    (0..k).filter(|i| m >> i & 1 == 1).map(|i| mods[i]).collect();
                let r = simulate_json(&json!({
                    "weapon": "braton_prime", "mods": on,
                    "enemy": "corrupted_heavy_gunner", "level": 150,
                    "runs": 6, "seed": 0x5EED, "duration": 10,
                    "metric": metric, "run_series": true,
                }));
                assert!(r.get("error").is_none(), "{r}");
                r[if metric == "dps" { "dps_runs" } else { "score_runs" }].clone()
            })
            .collect();
        shapley_json(&json!({ "participants": mods, "values": values }))
    }

    /// Serration scales every number the fight produces and Vital Sense only
    /// the crits: two separate multipliers, so under DPS the pair does not
    /// interact and each is worth what it is alone — to within the engine's
    /// damage quantization, which rounds each hit and so is not a product. The
    /// target is out of reach of a kill, because a kill re-rolls the fight.
    #[test]
    fn two_separate_buckets_on_the_simulator_do_not_interact() {
        let r = measure(&["serration", "vital_sense"], "dps");
        assert_eq!(r["ok"], true, "{r}");
        let i01 = r["interactions"][0][1]["log"].as_f64().unwrap();
        assert!(i01.abs() < 1e-3, "Serration × Vital Sense interacted: {}", r["interactions"][0][1]);
        let (phi, alone) = (logs(&r, "shapley"), logs(&r, "alone"));
        assert!((phi[0] - alone[0]).abs() < 1e-3 && (phi[1] - alone[1]).abs() < 1e-3, "{r}");
    }

    /// Serration and Heavy Caliber add into ONE base-damage bucket, so each
    /// is worth less beside the other: a negative interaction.
    #[test]
    fn two_cards_in_one_bucket_dilute_each_other() {
        let r = measure(&["serration", "heavy_caliber"], "dps");
        assert_eq!(r["ok"], true, "{r}");
        let i01 = r["interactions"][0][1]["log"].as_f64().unwrap();
        assert!(i01 < -0.05, "one bucket should dilute: {i01}");
        let sum: f64 = logs(&r, "shapley").iter().sum();
        let total = (r["full"].as_f64().unwrap() / r["empty"].as_f64().unwrap()).ln();
        assert!((sum - total).abs() < 1e-9);
    }
}
