#!/usr/bin/env python3
"""Build the Windows installer, named and stamped for a network-drive release.

WHAT THIS PRODUCES: `dist/WFSim-<date>.exe`, an NSIS installer that puts the
app in `%LOCALAPPDATA%` — no administrator prompt, on installation or on any
later update — plus the SHA-256 to publish beside it, and a copy of the source
archive AGPL requires to travel with a binary.

NO VERSION NUMBER TO DECIDE (owner, 2026-08-26). Windows insists on a version
field for its own upgrade bookkeeping, so it is derived from the build date and
nobody ever picks one. What identifies a build for a bug report is the COMMIT,
which the page's own footer already shows — the same rule the web build follows.

THE INSTALLER IS NOT THE UPDATE CHANNEL. It is downloaded once, from wherever
the link was posted; everything after that arrives through `release_desktop.py`
as files, silently. So this script runs rarely — only when the SHELL changes —
and that is by design: see desktop/src/update.rs.
"""
import datetime
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DESKTOP = ROOT / "desktop"
DIST = ROOT / "dist"
CARGO = pathlib.Path.home() / ".cargo" / "bin" / "cargo.exe"


def run(*cmd: str, **kw) -> subprocess.CompletedProcess:
    print("  $ " + " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, cwd=kw.pop("cwd", ROOT), text=True,
                       encoding="utf-8", errors="replace", **kw)
    if r.returncode != 0:
        sys.exit(f"failed: {' '.join(str(c) for c in cmd)}")
    return r


def main() -> None:
    today = datetime.date.today()
    # Windows wants MAJOR.MINOR.PATCH with each part under 65536, so the date
    # goes in as year.month.day rather than as one number.
    version = f"{today.year}.{today.month}.{today.day}"

    conf_path = DESKTOP / "tauri.conf.json"
    conf = json.loads(conf_path.read_text(encoding="utf-8"))
    original = conf_path.read_text(encoding="utf-8")
    conf["version"] = version
    conf_path.write_text(json.dumps(conf, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    try:
        print(f"\nbuilding WFSim {version}")
        run(str(CARGO), "tauri", "build", "--config", str(conf_path), cwd=DESKTOP)
    finally:
        conf_path.write_text(original, encoding="utf-8")

    built = list((DESKTOP / "target" / "release" / "bundle" / "nsis").glob("*.exe"))
    if not built:
        sys.exit("no installer was produced — look for the bundler's output above")
    src = max(built, key=lambda p: p.stat().st_mtime)

    DIST.mkdir(exist_ok=True)
    name = f"WFSim-{today:%Y%m%d}.exe"
    dest = DIST / name
    shutil.copy2(src, dest)

    body = dest.read_bytes()
    digest = hashlib.sha256(body).hexdigest()

    # A SIZE CHECK, because the failure it catches builds cleanly. With two
    # [[bin]] targets in the crate, Tauri bundled `updatekit` instead of the
    # app: a 0.3 MB installer that installs, runs, and is not WFSim. The
    # payload alone is 29 MB, so anything remotely small is that mistake.
    if len(body) < 20_000_000:
        sys.exit(
            f"the installer is only {len(body) / 1e6:.1f} MB — the payload alone is 29 MB. "
            "Tauri almost certainly bundled the wrong binary; check mainBinaryName."
        )

    # The source archive goes to the same place, because AGPL requires the
    # corresponding source to be offered wherever the binary is.
    sys.path.insert(0, str(ROOT / "scripts"))
    import release_desktop  # noqa: E402
    (DIST / "source.zip").write_bytes(release_desktop.source_zip())

    notes = DIST / "安装说明.txt"
    notes.write_text(
        "WFSim — Warframe 伤害计算器\n"
        "\n"
        f"版本  {today:%Y-%m-%d}\n"
        f"SHA-256  {digest}\n"
        "\n"
        "安装\n"
        "  双击 " + name + " 即可，安装到当前用户目录，不需要管理员权限。\n"
        "\n"
        "关于安全提示\n"
        "  本程序没有购买代码签名证书，Windows 首次运行会显示蓝色提示\n"
        "  「Windows 已保护你的电脑」。点击「更多信息」→「仍要运行」即可。\n"
        "  介意的话可以用上面的 SHA-256 自行校验，命令：\n"
        "      certutil -hashfile " + name + " SHA256\n"
        "\n"
        "更新\n"
        "  装好之后程序会自动更新，不需要再回到网盘下载。\n"
        "\n"
        "源码\n"
        "  source.zip 是本程序的完整源码（AGPL-3.0）。\n"
        "  在线版本： https://wfsim.app\n"
        "  QQ 群： 995078378\n",
        encoding="utf-8",
    )

    print("\n" + "=" * 60)
    print(f"installer  {dest}  ({len(body) / 1e6:.1f} MB)")
    print(f"sha256     {digest}")
    print(f"source     {DIST / 'source.zip'}")
    print(f"notes      {notes}")
    print("\nupload the three files in dist/ to the network drive.")


if __name__ == "__main__":
    main()
