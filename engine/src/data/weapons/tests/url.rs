/// **A WEAPON HAS A URL OF ITS OWN.** `/weapons/<Wiki_Name>` is the
/// English display name with a parenthesised qualifier stripped — the
/// qualifier is OURS rather than the page's ("Larkspur Prime (Atmosphere)"
/// is one wiki page with two stat columns and we ship the ground one).
///
/// TWO ENTRIES ON ONE SLUG IS A NAMING SLIP — with ONE exception, and it
/// is a rule rather than a list: a Kitgun chamber is two roster entries
/// (the slot is the weapon) and one wiki page. The site gives the lower id
/// the wiki name and the other its id (`url_slug` in build_site_app.py,
/// `urlSlug` in app.js), and `check_every_weapon_has_a_url` holds that. Any
/// other pair on one slug fails here.
#[test]
fn no_two_weapons_want_the_same_url() {
    use std::collections::BTreeMap;
    let mut by_slug: BTreeMap<String, Vec<&super::WeaponSpec>> = BTreeMap::new();
    for w in super::roster() {
        let slug = w.name.split(" (").next().unwrap_or(&w.name).replace(' ', "_");
        by_slug.entry(slug).or_default().push(w);
    }
    let chamber = |w: &super::WeaponSpec| {
        let r = w.kitgun.as_deref()?;
        crate::data::weapons::kitguns::chamber(r).map(|c| c.chamber.as_str())
    };
    let clashes: Vec<String> = by_slug
        .iter()
        .filter(|(_, ws)| ws.len() > 1)
        .filter(|(_, ws)| {
            let first = chamber(ws[0]);
            first.is_none() || ws.iter().any(|w| chamber(w) != first)
        })
        .map(|(slug, ws)| {
            let ids: Vec<&str> = ws.iter().map(|w| w.id.as_str()).collect();
            format!("/weapons/{slug} <- {}", ids.join(", "))
        })
        .collect();
    assert!(
        clashes.is_empty(),
        "two roster entries that are not one Kitgun chamber want one URL:\n{}",
        clashes.join("\n")
    );
    // …AND THE EXCEPTION IS USED: every chamber is two entries on one slug.
    let shared = by_slug.values().filter(|ws| ws.len() == 2 && chamber(ws[0]).is_some()).count();
    assert_eq!(shared, 6, "six chambers, each one page for its two slots");
}
