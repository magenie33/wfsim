#!/usr/bin/env python3
"""Build the desktop app's icon set from the wordmark SVG.

TWO THINGS MAKE THIS BETTER THAN A RESIZE.

Every size is rendered from the SVG AT THAT SIZE rather than downscaled from
one big raster. A browser laying out 16px text applies hinting — it snaps stems
to the pixel grid — and no resampling filter can recover that from a 512px
image. The difference is visible at 16 and 24, which is exactly where an icon
is hardest to read.

And the small sizes get a DIFFERENT PICTURE. The full "WFSim" wordmark is five
glyphs; below 32px they collapse into a smudge. A .ico holds one image per
size, which is what the format is for, so 16/24 carry a "WF"-only mark that
stays legible at any scale.

Sources: `web/src/static/logo.svg` (the wordmark, also the site favicon) and
`desktop/logo_small.svg` (the small-size mark). Requires Chrome and Pillow.
"""
import io
import pathlib
import struct
import subprocess
import sys

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
FULL_SVG = ROOT / "web" / "src" / "static" / "logo.svg"
SMALL_SVG = ROOT / "desktop" / "logo_small.svg"
OUT = ROOT / "desktop" / "icons"
CHROME = pathlib.Path(r"C:/Program Files/Google/Chrome/Application/chrome.exe")

# Below this the wordmark is unreadable and the WF mark takes over. Measured by
# rendering both and looking: at 24 "WFSim" is already a smudge, at 32 it reads.
SMALL_BELOW = 32
SIZES = [16, 24, 32, 48, 64, 128, 256]


def shoot(svg: pathlib.Path, size: int, dest: pathlib.Path) -> Image.Image | None:
    """One headless screenshot, or None if it came back blank."""
    if dest.exists():
        dest.unlink()
    subprocess.run(
        [str(CHROME), "--headless", "--disable-gpu", "--hide-scrollbars",
         f"--screenshot={dest}", f"--window-size={size},{size}",
         "--default-background-color=00000000", svg.as_uri()],
        capture_output=True, check=False,
    )
    if not dest.exists():
        return None
    img = Image.open(dest).convert("RGBA")
    # AN EMPTY RENDER IS THE FAILURE MODE THAT LOOKS LIKE SUCCESS: Chrome exits
    # 0 and writes a well-formed, fully transparent PNG. Discovered when
    # `128x128.png` shipped at 144 bytes with not one opaque pixel, and the .ico
    # carried that layer — the size Windows uses for large-icon views.
    opaque = sum(1 for px in img.get_flattened_data() if px[3] > 8)
    return img if opaque else None


def render(svg: pathlib.Path, size: int, dest: pathlib.Path) -> Image.Image:
    """Render one SVG at one exact pixel size, transparent background.

    Chrome renders this SVG at 16–64 and at 200+, and returns a blank image for
    everything between — reproducibly, unaffected by `--virtual-time-budget`,
    and with a zero exit code. Rather than chase that, the fallback renders
    large and resamples: the hinting this function exists for only matters at
    the small sizes, which are exactly the ones that work.
    """
    img = shoot(svg, size, dest)
    if img is not None:
        return img
    big = shoot(svg, 512, dest.with_name(f"fallback-{size}.png"))
    if big is None:
        sys.exit(f"chrome rendered nothing for {svg.name}, at {size}px or at 512px")
    print(f"       (rendered at 512 and resampled — chrome returns blank at {size})")
    return big.resize((size, size), Image.LANCZOS)


def build_ico(images: list[tuple[int, Image.Image]]) -> bytes:
    """A PNG-compressed .ico (Vista and later). One entry per size."""
    entries, blobs = [], []
    offset = 6 + 16 * len(images)
    for size, img in images:
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        data = buf.getvalue()
        d = 0 if size >= 256 else size          # 256 is encoded as zero
        entries.append(struct.pack("<BBBBHHII", d, d, 0, 0, 1, 32, len(data), offset))
        offset += len(data)
        blobs.append(data)
    return struct.pack("<HHH", 0, 1, len(images)) + b"".join(entries) + b"".join(blobs)


def nsis_art(mark: Image.Image) -> None:
    """The installer's header and sidebar images.

    NSIS takes BMP and nothing else, at exactly these sizes — it does not
    scale, and a mismatched image is simply not drawn. Both are composited onto
    the app's own dark plane (#0e1014) because BMP has no alpha, so "transparent
    background" here means "whatever colour I forgot to pick".

    It is a small thing and it is the first thing anyone sees of this project
    that is not a download warning.
    """
    plane = (14, 16, 20)

    side = Image.new("RGB", (164, 314), plane)
    logo = mark.resize((112, 112), Image.LANCZOS)
    side.paste(logo, (26, 42), logo)
    side.save(OUT / "sidebar.bmp")

    head = Image.new("RGB", (150, 57), plane)
    small = mark.resize((44, 44), Image.LANCZOS)
    head.paste(small, (97, 6), small)
    head.save(OUT / "header.bmp")
    print("sidebar.bmp (164x314) / header.bmp (150x57)")


def main() -> None:
    if not CHROME.exists():
        sys.exit(f"chrome not found at {CHROME}")
    OUT.mkdir(parents=True, exist_ok=True)
    tmp = OUT / ".render"
    tmp.mkdir(exist_ok=True)

    images = []
    for size in SIZES:
        svg = SMALL_SVG if size < SMALL_BELOW else FULL_SVG
        img = render(svg, size, tmp / f"{size}.png")
        images.append((size, img))
        print(f"  {size:>3}px  {svg.name}")

    ico = OUT / "icon.ico"
    ico.write_bytes(build_ico(images))
    check = Image.open(ico)
    print(f"\nicon.ico  {ico.stat().st_size / 1024:.1f} KB  sizes={sorted(check.ico.sizes())}")

    # Tauri's bundler wants these three by name; they are the same renders.
    by_size = dict(images)
    by_size[32].save(OUT / "32x32.png")
    by_size[128].save(OUT / "128x128.png")
    by_size[256].save(OUT / "128x128@2x.png")
    print("32x32.png / 128x128.png / 128x128@2x.png")

    nsis_art(render(FULL_SVG, 256, tmp / "mark.png"))

    for f in tmp.iterdir():
        f.unlink()
    tmp.rmdir()


if __name__ == "__main__":
    main()
