# WFSim CLI

The official single-machine client for WFSim volunteer computing. It loads
the site's signed native engine and data, follows engine updates and uses the
same public work, verification and contribution rules as every other client.

Download the native executable for your platform from the official CLI release.
Use the npm launcher when the CLI release manifest marks that release
`npm: true`:

```sh
npx wfsim@latest join
```

The native download needs neither Node.js nor Rust. The npm launcher needs
Node.js 22.23.4 or newer. Builds cover x86-64 Linux (glibc 2.31+), Windows,
Apple Silicon macOS and Intel macOS. An engine without a native slice for the
current platform waits for an available official build without accepting work.

For a Linux download, first run `chmod +x wfsim-x86_64-linux`, then
`./wfsim-x86_64-linux join`. On macOS use the matching `wfsim-aarch64-macos`
or `wfsim-x86_64-macos` filename in those commands. On Windows, PowerShell
runs `./wfsim-x86_64-windows.exe join`. Rename the file to `wfsim` (or
`wfsim.exe`) and put it on your PATH to use the shorter commands below.

```sh
wfsim join
wfsim join --range 10-50 --name "My server"
wfsim config --range 0-50
wfsim start
wfsim status
wfsim pause
wfsim resume
wfsim stop
wfsim login
```

The first join displays the volunteer statement and asks for consent. `--yes`
accepts that statement explicitly in a noninteractive setup. Login is optional.
`login` prints a browser address and code that can be approved on another
computer or phone. The first claim includes unclaimed contribution; changing
accounts preserves each account's already attributed contribution.

The default CPU range is 0–50%, in whole percentage points. The client lowers
its target when other processes need CPU and recovers gradually when they do
not. The lower bound is a requested floor; 100–100 requests all available
compute continuously. A 0–0 range keeps the client running without accepting
work. Battery operation and low available memory pause computing. The budget
is averaged over compute pieces; a long atomic engine call can delay a change.
The process also requests low OS priority.

`start` detaches from the terminal and writes `client.log` in the device
directory. On Linux, `wfsim service install` installs a systemd user service;
the command prints the lingering setup needed for operation after logout.
`wfsim service remove` removes it. Other platforms support `start` without an
automatic startup installer.

The host supervises one compute process. Engine updates replace that process
after its work and answers have drained, releasing native engine memory while
preserving the device identity, settings and durable pending answers.

The default data directory is:

| system | directory |
| --- | --- |
| Linux | `$XDG_DATA_HOME/wfsim/cli`, or `~/.local/share/wfsim/cli` |
| macOS | `~/Library/Application Support/WFSim/cli` |
| Windows | `%LOCALAPPDATA%\WFSim\cli` |

`--data-dir <directory>` or `WFSIM_CLI_HOME` selects another persistent device
directory. Keep it private and back it up: it contains the device credential,
settings and results awaiting acknowledgment. It is independent of npm's
cache and of the downloaded executable. Do not share the credential or run a
copied identity concurrently on different machines. Confirmed contribution is
stored by WFSim; the local files are not its authoritative ledger.

Completed answers are written before submission and removed only after the
server acknowledges or explicitly drops them. Connection failures back off.
Pause, zero budget and shutdown return unfinished work. `status --refresh`
also reads confirmed contribution still held anonymously by the device;
account contributions and device management are on `/compute`.

Build from source with `cargo build --release --locked --manifest-path
client/Cargo.toml`. This host is independent of the game engine workspace.
`engine-check <board-fixture.json>` measures a local fixture with the signed
official engine without taking or submitting live work.

The source archive beside each release is the corresponding source. Protocol
and third-party integration: [docs/CLIENTS.md](https://github.com/magenie33/wfsim/blob/main/docs/CLIENTS.md).
