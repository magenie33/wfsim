#!/usr/bin/env python3
"""Package the in-browser builder into site/ (docs/WASM.md phase 4).

Steps:
  1. cargo build --release -p wfsim-wasm --target wasm32-unknown-unknown
  2. wasm-bindgen --target no-modules  ->  site/app/pkg/
  3. wasm-opt -Oz (if available; optional)
  4. copy web/src/static/{index.html,app.js,style.css,worker.js,pol/} -> site/
  5. inject <script>window.WFSIM_WASM = true;</script> into the copied
     index.html — that flag flips app.js's api() from fetch to worker RPC.

wrangler already serves site/ at wfsim.app, so after this script the builder
lives at wfsim.app/ and every simulation runs on the visitor's own CPU.

Prereqs: rustup target add wasm32-unknown-unknown;
         cargo install wasm-bindgen-cli --version <matching Cargo.lock>.
"""

import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATIC = ROOT / "web" / "src" / "static"
APP = ROOT / "site"
WASM = ROOT / "target" / "wasm32-unknown-unknown" / "release" / "wfsim_wasm.wasm"


def run(*cmd: str) -> None:
    print("+", " ".join(cmd))
    subprocess.run(cmd, cwd=ROOT, check=True)


def main() -> None:
    run("cargo", "build", "--release", "-p", "wfsim-wasm", "--target", "wasm32-unknown-unknown")
    APP.mkdir(parents=True, exist_ok=True)
    run("wasm-bindgen", str(WASM), "--target", "no-modules", "--no-typescript",
        "--out-dir", str(APP / "pkg"))

    # Optional size pass — the app works without it.
    if shutil.which("wasm-opt"):
        bg = APP / "pkg" / "wfsim_wasm_bg.wasm"
        run("wasm-opt", "-Oz", "-o", str(bg), str(bg))
    else:
        print("(wasm-opt not found — skipping the size pass)")

    for name in ("app.js", "style.css", "worker.js"):
        shutil.copy2(STATIC / name, APP / name)
    shutil.copytree(STATIC / "pol", APP / "pol", dirs_exist_ok=True)

    html = (STATIC / "index.html").read_text(encoding="utf-8")
    flagged = re.sub(
        r"(\s*)(<script src=\"/app\.js\"></script>)",
        r"\1<script>window.WFSIM_WASM = true;</script>\1\2",
        html,
        count=1,
    )
    if flagged == html:
        sys.exit("index.html: <script src=\"app.js\"> anchor not found — flag not injected")
    (APP / "index.html").write_text(flagged, encoding="utf-8", newline="\n")

    size = (APP / "pkg" / "wfsim_wasm_bg.wasm").stat().st_size
    print(f"site/ ready — wasm {size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
