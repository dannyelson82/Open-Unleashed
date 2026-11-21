# PROGRESS — OpenUnleashed macOS + tvOS (`tvos-port` branch)

Status of every stage of `openunleashed-tvos-prompt.md` (+ `enhancements-plan.md`, `airpods-plan.md`,
`server-plan.md`, `hd-texture-plan.md`) on both platforms. Details of every change: `TVOS.md`.

Legend: ✅ done & verified · 🟡 partly / done but unverified · ⛔ blocked · 📺 **needs Apple TV** (real device, from Wednesday) · — not applicable

_Last updated: 2026-11-21_

## Environment (this Mac)
| Item | State |
|---|---|
| Homebrew | ✅ user-local in `~/homebrew` (standard `/opt/homebrew` install needs `sudo` = Mac password, not available). `eval "$(~/homebrew/bin/brew shellenv)"` added to `~/.zprofile` |
| Rust | ✅ rustup stable 1.100.0 (≥ 1.95), targets `aarch64-apple-tvos`, `aarch64-apple-tvos-sim` |
| CMake | ✅ 4.5.1 (Homebrew) |
| Xcode | ✅ 27.2, tvOS SDK 27.2, tvOS 27.2 Simulator runtime |
| Keep-awake | ✅ `caffeinate -dims` running in the background |
| Game data | ✅ extracted from your disc image (see "Game data" below) into `game-data/` (git-ignored) |

## Game data (from the disc image, 2026-11-21)
- Source: `~/Downloads/Need for Speed - Porsche Unleashed (USA)/` (`.bin` + `.cue`, one MODE1/2352 data track). **Left untouched**
  (read only; the `.cue`'s checksum was recorded before and after).
- `bchunk` (Homebrew) → a temporary ISO in the session scratch folder → `7zz` (7-Zip 26.06) extracted only what the engine needs.
  No installer unpacking was needed: the disc carries `GameData/` and `FEData/` directly (so `unshield` wasn't used — its
  Homebrew build also failed: "Command Line Tools are too outdated").
- Result `game-data/` (508 MB, 1,594 files): `GameData/`, `FEData/`, `version.txt`, `fe.txt` (+ `readme.txt`).
- Checked against `docs/en/game-data.md`: ✅ `GameData/` (needed) · ✅ `FEData/` (needed) · ✅ `version.txt` (network games) — but it says
  **3.2** (unpatched disc), the README's network note expects 3.5, so network games with a patched 3.5 PC copy won't match ·
  ✅ `fe.txt` (optional) · ⚠️ `nfs5.exe` (optional, intro-movie tables only) **not on the disc**: the disc has `Porsche.exe` +
  `PORSCHE.ICD` + `secdrv.sys`/`clcd32.dll` (SafeDisc copy protection — the real game code is encrypted), so it's not
  copied; the intro movies are skipped cleanly, as documented.
- `.gitignore` now also blocks `/game-data/`, `/game/`, `GameData/`, `FEData/`, `*.bin`, `*.cue`, `*.iso`, `nfs5.exe`.
- Use: `export NFS5_GAME_PATH=~/Documents/OpenUnleashed/game-data`.

## Placeholders left for later (as asked)
`<UNRAID_IP_OR_HOSTNAME>` / `<TAILSCALE_NAME>`, `<MY_APPLE_ID_OR_TEAM>`,
Apple TV paired `<yes/no>`, controller `<Xbox / PlayStation / other>`, bundle ID `<com.yourname.openunleashed>`
(a stand-in `org.openunleashed.tvos` is used until then).

## Stage status
| Stage | macOS | tvOS Simulator | Apple TV |
|---|---|---|---|
| 1 Baseline build | ✅ release builds; runs; without data exits with the clear "game data not found" error | — | — |
| 1 Race starts | ✅ `NFS5_GAME_PATH=game-data open-unleashed alps 993`: 20 s headless race (car accelerates, shifts 1→4); windowed `--screenshot` race with 3 opponents renders correctly on Metal (Apple M2 Pro) | — | — |
| 1 Engine as a library | ✅ `crates/open-unleashed` is now `lib.rs` + a 3-line `main.rs`; `run()` split into `build_app()` + event loop (desktop order unchanged) | ✅ builds as a static lib | — |
| 2 tvOS cross-compile | ✅ unaffected | ✅ `cargo build --release --target aarch64-apple-tvos-sim -p open-unleashed --lib` succeeds (winit 0.30 patched, SDL3 video on for tvOS) | 🟡 device target `aarch64-apple-tvos` not built yet |

## Needs Apple TV
(filled in as stages progress)

## Blockers
(filled in as they appear)
