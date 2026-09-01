#!/usr/bin/env python3
"""Build the Windows download: one executable, plus the source AGPL asks for.

WHAT LANDS IN `dist/`:

  WFSim.exe      the whole app. The 29 MB payload is an `include_bytes!` inside
                 the binary, unpacked to %LOCALAPPDATA% on first run — so there
                 is nothing to unpack, nothing to install, no administrator
                 prompt, and deleting the file is the uninstall.
  source.zip     what AGPL requires to travel with a binary.
  使用说明.txt    which of those to download, and that the source is not
                 something a player needs.

NO INSTALLER. It was an NSIS bundle for a while, worth
9 MB of compression and a Start menu shortcut. Dropping it also drops the
BUNDLER, which is where both of this project's "builds cleanly, ships the wrong
thing" failures came from: it packaged the signing tool instead of the app, and
it truncated a 33.7 MB binary to 288 KB while patching a marker into it. What
cargo produces is now the artifact, unmodified, and the build is a minute
shorter. The icon is unaffected — `tauri_build` writes it into the Windows
resource from `build.rs`, which the bundler was never involved in.

NO VERSION NUMBER TO DECIDE. Windows still wants a version field for its own
bookkeeping, so it is derived from the build date and nobody ever picks one.
What identifies a build for a bug report is the COMMIT, which the page's own
footer already shows — the same rule the web build follows. The date does not
reach the filename or the notes: see the comment above the notes for why
putting it there was actively misleading.

THE EXE IS NOT THE UPDATE CHANNEL. It is downloaded once, from wherever the
link was posted; everything after that arrives through `release_desktop.py` as
files, silently. So this script runs rarely — only when the SHELL changes — and
that is by design: see desktop/src/update.rs.
"""
import datetime
import hashlib
import os
import json
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DESKTOP = ROOT / "desktop"
DIST = ROOT / "dist"


def cargo() -> str:
    """`cargo` from PATH, falling back to the rustup default location.

    On this machine cargo is installed but not on PATH; on a CI runner the
    toolchain action puts it on PATH and nowhere predictable. Asking PATH first
    covers both without either needing to know about the other.
    """
    found = shutil.which("cargo")
    if found:
        return found
    local = pathlib.Path.home() / ".cargo" / "bin" / ("cargo.exe" if os.name == "nt" else "cargo")
    if local.exists():
        return str(local)
    sys.exit("cargo not found on PATH or in ~/.cargo/bin")


def run(*cmd: str, **kw) -> subprocess.CompletedProcess:
    print("  $ " + " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, cwd=kw.pop("cwd", ROOT), text=True,
                       encoding="utf-8", errors="replace", **kw)
    if r.returncode != 0:
        sys.exit(f"failed: {' '.join(str(c) for c in cmd)}")
    return r


def main() -> None:
    # THIS SCRIPT PRINTS A CHINESE FILENAME, and a Windows stdout that is not a
    # console defaults to the ANSI code page — cp1252 on a GitHub runner, where
    # `使用说明.txt` is not encodable. It built the whole 37.9 MB executable and
    # then died on the last line of its own summary, which reads from outside
    # as a release that failed to build.
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")

    today = datetime.date.today()
    # Windows wants MAJOR.MINOR.PATCH with each part under 65536, so the date
    # goes in as year.month.day rather than as one number.
    version = f"{today.year}.{today.month}.{today.day}"

    conf_path = DESKTOP / "tauri.conf.json"
    conf = json.loads(conf_path.read_text(encoding="utf-8"))
    original = conf_path.read_text(encoding="utf-8")
    conf["version"] = version
    conf_path.write_text(json.dumps(conf, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    windows = os.name == "nt"
    try:
        print(f"\nbuilding WFSim {version}")
        if windows:
            # PLAIN CARGO ON WINDOWS. `tauri build` would produce an installer
            # this project no longer ships, and its post-build patching step is
            # what silently truncated this binary from 33.7 MB to 288 KB once
            # already. What cargo writes is the artifact.
            run(cargo(), "build", "--release", "--manifest-path",
                str(DESKTOP / "Cargo.toml"), "--bin", "wfsim-desktop")
        else:
            # THE BUNDLER EARNS ITS KEEP ON LINUX, where a bare binary links
            # against whatever WebKitGTK the distribution happens to ship and an
            # AppImage carries its own. That is the difference between "download
            # and run" and "download, then install the right webkit package" —
            # and it gives the file a real extension, which a bare ELF has not.
            #
            # Neither Windows failure applies here: the crate has exactly one
            # binary now (updatekit is an example), so there is nothing to pick
            # wrongly, and the size check below still catches a truncation.
            run(cargo(), "tauri", "build", "--bundles", "appimage", cwd=DESKTOP)
    finally:
        conf_path.write_text(original, encoding="utf-8")

    DIST.mkdir(exist_ok=True)

    # NO DATE, NO VERSION, IN THE FILENAME. A network drive
    # share link is tied to the file, so renaming the download invalidates every
    # link already posted — and this is the one artifact that almost never needs
    # replacing, since everything after it arrives through the update channel. A
    # stable name is worth more than being able to tell two downloads apart,
    # which the SHA-256 does anyway.
    #
    # EACH PLATFORM'S OWN EXTENSION, and nothing else to tell them apart: `.exe` and `.AppImage` both say what they are to the
    # system and to the reader, where `WFSim-linux` was a label we invented.
    if windows:
        built = DESKTOP / "target" / "release" / "wfsim-desktop.exe"
        dest = DIST / "WFSim.exe"
    else:
        found = sorted((DESKTOP / "target" / "release" / "bundle" / "appimage").glob("*.AppImage"))
        if not found:
            sys.exit("no AppImage was produced — look for the bundler's output above")
        built = found[-1]
        dest = DIST / "WFSim.AppImage"
    shutil.copy2(built, dest)
    if not windows:
        # An asset downloaded from a Release arrives without the execute bit;
        # the release body says `chmod +x`, and this makes the local copy run.
        dest.chmod(0o755)
    body = dest.read_bytes()
    digest = hashlib.sha256(body).hexdigest()

    # A SIZE CHECK, because what it catches builds cleanly and exits zero.
    # `lto = true` once dropped the whole 29 MB payload as unreachable, leaving
    # a 268 KB binary that ran and unpacked nothing. The payload alone is 29 MB,
    # so anything remotely small is that class of mistake.
    if len(body) < 20_000_000:
        sys.exit(
            f"the artifact is only {len(body) / 1e6:.1f} MB — the payload alone is 29 MB. "
            "The app was almost certainly optimised out (check that lto is off) "
            "or the bundler packaged the wrong thing."
        )

    # The source archive goes to the same place, because AGPL requires the
    # corresponding source to be offered wherever the binary is.
    sys.path.insert(0, str(ROOT / "scripts"))
    import release_desktop  # noqa: E402
    (DIST / "source.zip").write_bytes(release_desktop.source_zip())

    # NO BUILD DATE IN THE NOTES. It was there so a second
    # upload could be told from the first, which serves the person uploading and
    # costs the person downloading: a file stamped August, read in December,
    # looks stale — and it is not, because the client updates itself on its
    # first run whichever copy was downloaded. The date said something untrue.
    # The SHA-256 tells the two apart, and more precisely.
    # THE WORDING IS THE OWNER'S, edited by hand in `dist/` and
    # brought back here so the next build does not overwrite it. Only the
    # SHA-256 is generated — everything else is his text, kept verbatim.
    # THE NOTES ARE FOR THE NETWORK DRIVE, which is a Windows audience: they
    # explain SmartScreen and `certutil`, neither of which exists elsewhere. The
    # Linux download goes out through a GitHub Release, where the release body
    # is the place to say `chmod +x` — so this file is simply not written there
    # rather than translated into something half-true.
    if not windows:
        print("\n" + "=" * 60)
        print(f"app        {dest}  ({len(body) / 1e6:.1f} MB)")
        print(f"sha256     {digest}")
        print(f"source     {DIST / 'source.zip'}")
        return

    notes = DIST / "使用说明.txt"
    notes.write_text(
        "WFSim: 终极 Warframe 计算器\n"
        "\n"
        "怎么用\n"
        "下载 WFSim.exe，双击打开。\n"
        "程序会自动更新，不需要再次下载。\n"
        "source.zip 是源代码，开源许可证要求随程序一起提供，正常使用不需要下载。\n"
        "\n"
        "第一次打开\n"
        "Windows 会弹出蓝色提示「Windows 已保护你的电脑」，\n"
        "因为本程序没有购买代码签名证书（一年好几千，一个免费工具不值得）。\n"
        "点「更多信息」→「仍要运行」即可。\n"
        "如果杀毒软件报警，也是同样的原因（没有签名的新程序）。\n"
        "介意的话可以校验下面的 SHA-256，或者直接用在线版。\n"
        "\n"
        "校验步骤（可选）\n"
        "在文件所在文件夹按住 Shift 右键 →「在此处打开终端」，然后：\n"
        "certutil -hashfile WFSim.exe SHA256\n"
        f"应为 {digest}\n"
        "\n"
        "卸载\n"
        "删掉 WFSim.exe 即可。\n"
        "程序数据在 %LOCALAPPDATA%\\WFSim，可一并删除。不写注册表。\n"
        "\n"
        "其他\n"
        "官网: https://wfsim.app\n"
        "GitHub: https://github.com/magenie33/wfsim\n"
        "QQ 群: 995078378\n"
        "Discord: https://discord.gg/5GXgbtmxY\n"
        "许可证: AGPL-3.0\n",
        encoding="utf-8",
    )
    # Names from when this shipped an installer as well.
    for stale in ("安装说明.txt", "WFSim-安装版.exe"):
        p = DIST / stale
        if p.exists():
            p.unlink()

    print("\n" + "=" * 60)
    print(f"app        {dest}  ({len(body) / 1e6:.1f} MB)")
    print(f"sha256     {digest}")
    print(f"source     {DIST / 'source.zip'}")
    print(f"notes      {notes}")
    print("\nupload WFSim.exe, source.zip and the notes to the network drive.")


if __name__ == "__main__":
    main()
