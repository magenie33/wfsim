# M129 — glaive charge and motion use separate clocks and recall at their outbound speed (owner, 2026-10-28)

The owner's in-game observations and the fight's input policy, without a
stopwatch reading of the absolute throw cycle:

- The charge takes wind-up speed; the release motion takes attack speed.
  These are separate segments, including on glaive throws.
- Tapping melee recalls the disc; pressing heavy detonates it. Both return the
  disc at the speed it flew out, including projectile-speed bonuses.
- Catching the disc adds no recovery before the next throw can start.
- The first throw leads into the consecutive-throw loop; consecutive throws
  then repeat without returning to the first-throw animation.

The fight assumes an immediate recall or detonation input at contact, with no
reaction delay. Outbound and return travel still take time. The ordinary throw
mode recalls; the detonation mode detonates. An explicit list with neither
input leaves the glaive to fly and bounce until it returns.

The ordinary Glaive's charged throw uses the extracted charge parameter and
release events. Its first release leads into alternating consecutive releases.
Flight begins at the release event and overlaps the remaining animation; the
next throw waits for both that animation and the return to finish. Other throw
forms retain their declared untimed release until their timelines are entered.

Tests: `fight::tests::glaive` and `fight::tests::timed_melee`.
