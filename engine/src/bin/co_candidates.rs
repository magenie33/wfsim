//! WHICH WEAPON WOULD SETTLE IT — a ranked shortlist for measuring whether an
//! Incarnon evolution's flat base damage feeds the GunCO term.
//!
//! The catalog's rule is ABSENCE MEANS ORDINARY, and MEASUREMENTS M49 is the
//! first row where absence turned out not to mean that. This prints every
//! weapon+perk pair the question can be asked of, ordered by how far apart the
//! two hypotheses land — because a perk that adds 20 to a base of 1200 answers
//! nothing no matter how carefully it is measured.
//!
//!   cargo run --release --bin co_candidates
use wfsim_engine::loadout::{CoBehavior, WeaponBase};

fn main() {
    // A REALISTIC MEASURING BUILD: Galvanized Shot / Aptitude at 3 stacks
    // against 2 status types, which is 40% x 3 x 2.
    let k = 0.4 * 3.0 * 2.0;

    let mut rows: Vec<(f64, String)> = Vec::new();
    for w in wfsim_engine::weapons_data::all() {
        // EVOLUTIONS ARE KEYED ON THE GROUP'S DEFAULT FORM, and the catalog's
        // rows are usually written against the INCARNON one ("Dual Toxocyst |
        // Incarnon Mode"), which is a separate weapon entry. So each entry asks
        // its group's default for the perk list and then resolves it against
        // ITSELF — otherwise every Incarnon form is silently skipped, which is
        // exactly the half the question is about.
        let forms = wfsim_engine::weapons_data::forms_of(&w.id);
        let Some(key) = forms.iter().find(|f| f.is_default).map(|f| f.weapon_id) else { continue };
        for tier in 1..=5u32 {
            for e in wfsim_engine::evolutions_data::options(key, tier) {
                let with = WeaponBase::from_data(&w.id, false, &[&e.id]);
                let bare = WeaponBase::from_data(&w.id, false, &[]);
                let (evolved, orig) = (with.base_vector.total(), bare.base_vector.total());
                // Only perks that RAISE the base can be asked the question.
                if evolved <= orig + 1e-9 {
                    continue;
                }
                let f = orig / evolved;
                // The two hypotheses, as the damage a measuring shot reads.
                let (included, excluded) = match with.co_behavior {
                    // The CO chunk is added to the base bucket.
                    CoBehavior::AdditiveWithBaseDamage => (evolved * (1.0 + k), evolved + orig * k),
                    // A pure multiplier, so the fraction scales the multiplier.
                    CoBehavior::Independent => (evolved * (1.0 + k), evolved * (1.0 + k * f)),
                    CoBehavior::Inert => continue,
                };
                let spread = included / excluded - 1.0;
                let flagged = with.co_base_fraction < 0.999;
                rows.push((
                    spread,
                    format!(
                        "{:>6.1}%  {:<26} {:<34} {:>6.0} -> {:<6.0} f={:.3}  {:<12} {}",
                        spread * 100.0,
                        w.id,
                        e.id,
                        orig,
                        evolved,
                        f,
                        match with.co_behavior {
                            CoBehavior::AdditiveWithBaseDamage => "adding",
                            CoBehavior::Independent => "MULTIPLYING",
                            CoBehavior::Inert => "inert",
                        },
                        if flagged { "[already excluded]" } else { "" }
                    ),
                ));
            }
        }
    }
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!(
        "spread   weapon                     perk                               base -> evolved        behavior     status"
    );
    for (_, line) in &rows {
        println!("{line}");
    }
    println!("\n{} candidate pairs", rows.len());
}
