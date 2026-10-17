// SPDX-License-Identifier: AGPL-3.0-or-later
//! AN INSTALLED PACK REPLACES WHAT THE BUILD EMBEDS OF IT, and nothing else
//! (docs/WASM.md §"Code and data"). Its own process: the first read of the
//! data seals it, and every other test reads it.

use wfsim_engine::data;

#[test]
fn an_installed_pack_replaces_its_embedded_files_and_the_first_read_seals_it() {
    let tiny = "weapons: {}\n".to_string();
    assert_eq!(data::install("site", vec![("assets.yaml".into(), tiny.clone())]), Ok(1));
    assert!(data::install("site", vec![]).unwrap_err().contains("already installed"));
    assert!(data::missing_packs().is_empty(), "a build that embeds waits for nothing");

    // THE WHOLE PACK IS REPLACED: its other embedded files are gone with it…
    assert_eq!(data::file("assets.yaml"), Some(tiny.as_str()));
    assert_eq!(data::file("market.yaml"), None);
    // …and every other pack is the build's own.
    assert!(data::file("weapons/primary/acceltra.yaml").is_some());
    assert!(data::files_under("i18n/").next().is_some());

    assert!(data::install("text", vec![]).unwrap_err().contains("read before it was installed"));
}
