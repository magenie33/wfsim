## What this changes

<!-- One or two sentences. If it moves a number, say which number and why the
     new one is right. -->

## Why it is right

<!-- A source, a measurement, or a test. A faithful-looking implementation
     without something to compare it against does not count as correct.
     Engine change? Name the golden test or the M-number behind it. -->

## Checklist

- [ ] I have read and agree to the [CLA](../CLA.md), and have signed it by
      commenting `I have read the CLA and I hereby sign the CLA` below
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] Golden values are unchanged, or a measurement justifying the change is
      cited above
- [ ] Frontend change? `web/src/static/` edits need `wfsim-web` rebuilt to be
      visible, and `site/` regenerated before shipping

<!-- Small and focused merges fastest — see CONTRIBUTING.md on the pace. -->
