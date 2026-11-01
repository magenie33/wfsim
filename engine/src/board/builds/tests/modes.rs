use super::*;

#[test]
fn board_modes_follow_the_seated_or_bare_timeline() {
    let build = |weapon: &str, stance: Option<&str>| {
        super::super::validate(weapon, &stance.into_iter().map(str::to_string).collect::<Vec<_>>(), &[], &[], "")
            .unwrap()
    };
    let modes = |b: ValidBuild| b.board_modes().iter().map(|m| m.id).collect::<Vec<_>>();
    let all = ["base", "forward", "block", "block_forward", "slide", "heavy", "heavy_slam"];
    for weapon in ["magistar", "praedos"] {
        let found = modes(build(weapon, None));
        assert_eq!(found.len(), all.len(), "{weapon}: {found:?}");
        assert!(all.iter().all(|id| found.contains(id)), "{weapon}: {found:?}");
    }
    let bare = modes(build("corufell_prime", None));
    assert!(!bare.contains(&"block") && !bare.contains(&"block_forward"), "{bare:?}");
    assert!(bare.contains(&"heavy") && bare.contains(&"heavy_slam"), "{bare:?}");
    let stance = modes(build("magistar", Some("crushing_ruin")));
    assert_eq!(stance.len(), all.len());
    assert_eq!(modes(build("gotva_prime", None)), ["base"]);
}
