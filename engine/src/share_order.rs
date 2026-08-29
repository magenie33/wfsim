//! THE SHARE MANIFEST — a frozen, APPEND-ONLY order for every id a share link
//! can name.
//!
//! A share link used to spell its ids out (`galvanized_diffusion`), which is
//! why a build cost about 280 characters even after deflate: the payload is
//! mostly identifiers, and deflate cannot know the ones the payload does NOT
//! contain. Sending an INDEX instead takes the same build to about 76.
//!
//! **THE PRICE IS THIS FILE, and the repo's own rule named it before it was
//! paid**: *"IDs travel as their own stable slugs, never as indices into a
//! table: a table would have to stay append-only forever or silently
//! reinterpret every link already posted"* (AGENTS.md).
//!
//! So the table stays append-only forever — and that is a RATCHET rather than a
//! promise. `scripts/gen_share_order.py` only ever appends; the test below
//! recomputes the digest over the whole list and fails on anything that is not
//! an append, so a reorder is a red test rather than a link that quietly opens
//! somebody else's build.
//!
//! THE INDEX REACHES THE PAGE THROUGH `/api/meta`, one `si` per entity, rather
//! than as a copy of this list: the page already receives every weapon, mod,
//! arcane and evolution, so the order can ride on what is already travelling.

use std::collections::HashMap;
use std::sync::OnceLock;

/// FNV-1a over the joined list. Written out rather than borrowed from `std`
/// because the PYTHON generator computes the same number from the same bytes,
/// and `DefaultHasher` guarantees nothing across languages or versions.
#[cfg(test)]
fn fnv1a(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

struct Manifest {
    ids: Vec<&'static str>,
    index: HashMap<&'static str, u32>,
    /// What the generator computed over `ids`. Read only by the RATCHET, which
    /// is the whole reason it is written down — production never looks at it.
    #[cfg_attr(not(test), allow(dead_code))]
    digest: String,
}

fn manifest() -> &'static Manifest {
    static M: OnceLock<Manifest> = OnceLock::new();
    M.get_or_init(|| {
        let text = crate::data::file("share_order.yaml")
            .expect("data/share_order.yaml — run scripts/gen_share_order.py");
        let mut ids: Vec<&'static str> = Vec::new();
        let mut digest = String::new();
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("digest:") {
                digest = rest.trim().to_string();
            } else if let Some(rest) = t.strip_prefix("- ") {
                ids.push(rest.trim());
            }
        }
        let index = ids.iter().enumerate().map(|(i, id)| (*id, i as u32)).collect();
        Manifest { ids, index, digest }
    })
}

/// Where this id sits in the frozen order, or `None` for one the manifest has
/// never seen — which is what a mod added since the last generation looks like,
/// and why a share falls back to spelling it out rather than refusing.
pub fn index_of(id: &str) -> Option<u32> {
    manifest().index.get(id).copied()
}

/// The id at that index, for the other direction.
pub fn id_at(i: u32) -> Option<&'static str> {
    manifest().ids.get(i as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **THE RATCHET.** Reordering or deleting a line reinterprets every link
    /// already posted, silently and forever — the exact failure the rule that
    /// forbade index tables was written about. This makes it a red test.
    ///
    /// The digest is computed by the GENERATOR in Python and re-computed here in
    /// Rust from the same bytes, so the two cannot drift: a hand edit that
    /// forgets to re-run the generator fails, and so does a generator that
    /// starts producing a different order.
    #[test]
    fn the_share_manifest_is_what_it_says_it_is() {
        let m = manifest();
        assert!(!m.ids.is_empty(), "the share manifest is empty");
        let want = format!("{:016x}", fnv1a(&m.ids.join(",")));
        assert_eq!(
            m.digest, want,
            "the share manifest's digest does not match its contents — if you \
             APPENDED, re-run scripts/gen_share_order.py; if you REORDERED or \
             DELETED, do not: every link already posted names these by index"
        );
    }

    /// A DUPLICATE WOULD MAKE TWO BUILDS ONE, and the second id would be
    /// unreachable — a mod nobody could ever share.
    #[test]
    fn no_id_is_listed_twice() {
        let m = manifest();
        let uniq: std::collections::BTreeSet<_> = m.ids.iter().collect();
        assert_eq!(uniq.len(), m.ids.len(), "duplicate id in the share manifest");
    }

    /// EVERY ID A BUILD CAN CARRY IS IN IT. A mod added without re-running the
    /// generator is a mod a share link has to spell out — which still works, but
    /// silently costs every link that carries it, so it is reported here rather
    /// than discovered by somebody measuring a URL.
    #[test]
    fn every_shareable_id_is_frozen() {
        let mut missing: Vec<String> = Vec::new();
        for w in crate::weapons_data::all() {
            if index_of(&w.id).is_none() {
                missing.push(format!("weapon {}", w.id));
            }
        }
        // THE MODS EVERY WEAPON CAN HOLD, which is every mod the roster
        // reaches — there is no flat "all mods" accessor and asking per weapon
        // is the same set by construction.
        let mut seen = std::collections::BTreeSet::new();
        for w in crate::weapons_data::all() {
            for m in crate::mods_data::pool_for_weapon(&w.id) {
                if seen.insert(m.id) && index_of(m.id).is_none() {
                    missing.push(format!("mod {}", m.id));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "not in data/share_order.yaml — run scripts/gen_share_order.py:\n{}",
            missing.join("\n")
        );
    }

    /// The two directions agree, which is the whole contract a link rests on.
    #[test]
    fn an_index_round_trips() {
        for id in ["serration", "galvanized_diffusion", "laetum"] {
            let i = index_of(id).unwrap_or_else(|| panic!("{id} not frozen"));
            assert_eq!(id_at(i), Some(id));
        }
        assert_eq!(id_at(u32::MAX), None);
    }
}
