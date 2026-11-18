## What this changes

<!-- One or two sentences. If it moves a number, say which number and why the
     new one is right. -->

## Why it is right

<!-- A source, a measurement, or a test. A faithful-looking implementation
     without something to compare it against does not count as correct.
     Engine change? Name the golden test or the M-number behind it. -->

## Checklist

- [ ] I signed using [CLA §7](https://github.com/magenie33/wfsim/blob/main/CLA.md#7-how-to-sign), or my existing signature covers this contribution.
- [ ] I have identified any co-authors and the source and licence of any third-party material.
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] Golden values are unchanged, or a measurement justifying the change is
      cited above
- [ ] Frontend change? `web/src/static/` edits need `wfsim-web` rebuilt to be
      visible, and `site/` regenerated before shipping

<!-- Small and focused merges fastest — see CONTRIBUTING.md on the pace. -->
