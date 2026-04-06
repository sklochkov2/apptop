# apptop

A `top`-like terminal utility that aggregates memory usage **by application** rather than by individual process. Many desktop applications (browsers, Electron apps, etc.) spawn multiple processes that share memory — `apptop` reads `/proc/<pid>/smaps_rollup` and rolls up proportional set size (PSS) and swap usage per executable, giving you one line per application.

## Building

```bash
make build          # debug build
make release        # optimized release build
```

Or directly with Cargo:

```bash
cargo build --release
```

## Installation

```bash
sudo make install           # installs to /usr/local/bin
sudo make PREFIX=/usr install   # installs to /usr/bin
```

## Usage

```
apptop [OPTIONS]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-d`, `--delay <SECS>` | Refresh interval in seconds | `2.0` |
| `-b`, `--batch` | Batch mode (non-interactive, prints to stdout) | off |
| `-n`, `--iterations <N>` | Number of iterations (implies batch mode; 0 = unlimited) | `0` |
| `-s`, `--sort <COL>` | Sort column: `pss`, `swap`, `total`, `procs`, `name` | `total` |

### Interactive Keys

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `s` | Cycle sort column |
| `r` | Reverse sort order |
| `1`–`5` | Sort by column (1=NPROC, 2=PSS, 3=SWAP, 4=TOTAL, 5=COMMAND) |
| `↑`/`k`, `↓`/`j` | Scroll up / down |
| `PgUp` / `PgDn` | Page up / down |
| `Home` / `End` | Jump to top / bottom |

### Examples

```bash
# Interactive TUI, refresh every 3 seconds, sorted by PSS
apptop -d 3 -s pss

# Batch: print a single snapshot and exit
apptop -b -n 1

# Batch: print every 5 seconds until interrupted
apptop -b -d 5
```

## How It Works

For every numeric directory in `/proc` (i.e. every process), `apptop`:

1. Resolves `/proc/<pid>/exe` to get the executable path.
2. Reads `/proc/<pid>/smaps_rollup` and extracts **Pss** and **SwapPss** (in KiB).
3. Aggregates these values by executable path.
4. Displays sorted results in a terminal UI or batch output.

**PSS** (Proportional Set Size) fairly splits shared pages among all processes that map them, so the sum across all applications gives a realistic picture of total physical memory consumption.

## Requirements

- Linux (uses `/proc` filesystem)
- Rust 2024 edition (1.85+)

## License

MIT
