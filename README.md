# syscheck (Rust) — v2.0

Terminal system performance checker for **Linux** — native Rust rewrite of the
Python version, with a full ratatui TUI dashboard and a fully portable build.

## What it shows

- **CPU** — overall + per-core usage, model, clock speed
- **Memory** — RAM and swap usage (`MemAvailable`-based)
- **Storage** — per-filesystem capacity/inode usage, per-disk I/O
  (throughput, IOPS, % utilization)
- **Network** — per-interface up/down throughput and link state
- **Top processes** — top 10 by CPU / by RAM
- **Temperatures** — thermal zones / hwmon sensors
- **Speed test** — internet ping/download/upload (Cloudflare, optional)

All data is read directly from `/proc` and `/sys` — nothing is shelled out.

## Usage

```bash
./target/release/syscheck                  # one-shot text report
./target/release/syscheck --watch          # full-screen TUI dashboard (2s)
./target/release/syscheck --watch 1        # 1s refresh
./target/release/syscheck --only cpu,mem   # restrict sections
./target/release/syscheck --speedtest      # + internet speed test
./target/release/syscheck --json           # machine-readable output
```

Sections: `cpu`, `mem`, `disk`, `net`, `procs`, `temp`.

### TUI keys

| Key | Action |
|---|---|
| `q` / `Esc` / `Ctrl+C` | quit |
| `+` / `-` | faster / slower refresh |
| `c` / `m` | sort processes by CPU / memory |
| `s` | run internet speed test (background) |

## Portable build ("any Linux machine")

```bash
./build-static.sh
```

Produces in `dist/`:

- **`syscheck`** — fully static musl binary (~3 MB). Copy to *any* x86_64
  Linux (Debian, Ubuntu, Fedora, Arch, Alpine, containers/servers without
  libc), `chmod +x`, run. Zero dependencies, no installer.
- **`syscheck-x86_64.AppImage`** — same binary packaged as a desktop
  application with icon + `.desktop` entry. Run it directly (needs FUSE)
  or extract: `./syscheck-x86_64.AppImage --appimage-extract-and-run`.

Requirements for the static build: `musl-tools` (`sudo apt-get install
musl-tools`) — the TLS library (`ring`) compiles C code for musl.

## Development

```bash
cargo build --release       # regular glibc build
cargo run -- --only cpu     # quick test
python3 packaging/make_icon.py packaging/syscheck.png   # regenerate icon
```

## Notes

- The speed test uses rustls with baked-in webpki roots — no system CA
  certificates needed, so HTTPS works even on bare container images.
- Colors are disabled automatically when output is piped; `--json` is
  script-friendly.
- One-shot mode samples over a 0.5 s window to compute rates; the TUI
  uses the current refresh interval as the sampling window.
