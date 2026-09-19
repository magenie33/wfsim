# M92 — Catalyzing Shields gives its full gate on ANY shield refill ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

With **Catalyzing Shields** equipped, the shield gate after a refill lasts **1.33 s
whatever the size of the refill** — a few shields restored by casting gives the
same gate as a full bar.

### What it settles

The wiki states both answers:

- W`Catalyzing_Shields`: it "sets the Shield Gating duration to a fixed value upon
  recovering any amount of Shields", and W`Shield` agrees ("1.33 seconds upon
  recovering any amount of shields").
- The Update 34 notes on W`Shield`: "Shield Gating duration scales from 0.33 to
  1.33 based on your maximum Shield values", with 25 of 100 shields at break
  giving 0.34 s.

The game follows the mod page. A partial refill under Catalyzing Shields is the
fixed value; the scaling in the Update 34 notes does not apply to it.

### Where it is read

`engine::data::warframes::resolve` — a cast's gate is Catalyzing Shields' fixed
seconds whenever the card is seated, and W`Shield`'s formula at the refilled
shields otherwise. `the_shield_gate_follows_the_shield_page_and_catalyzing_shields`
holds it with a 10-of-37 refill.
