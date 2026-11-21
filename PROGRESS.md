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

## Placeholders
Filled in: `<UNRAID_IP_OR_HOSTNAME>` = **192.168.50.37 / tower.local**, `<TAILSCALE_NAME>` = **tower-1.tawny-taipan.ts.net**
(used in `docs/ou-server.md`; the game's Server setting will default to `tower.local`).

Left for later (as asked): `<MY_APPLE_ID_OR_TEAM>`,
Apple TV paired `<yes/no>`, controller `<Xbox / PlayStation / other>`, bundle ID `<com.yourname.openunleashed>`
(a stand-in `org.openunleashed.tvos` is used until then).

## Stage status
| Stage | macOS | tvOS Simulator | Apple TV |
|---|---|---|---|
| 1 Baseline build | ✅ release builds; runs; without data exits with the clear "game data not found" error | — | — |
| 1 Race starts | ✅ `NFS5_GAME_PATH=game-data open-unleashed alps 993`: 20 s headless race (car accelerates, shifts 1→4); windowed `--screenshot` race with 3 opponents renders correctly on Metal (Apple M2 Pro) | — | — |
| 1 Engine as a library | ✅ `crates/open-unleashed` is now `lib.rs` + a 3-line `main.rs`; `run()` split into `build_app()` + event loop (desktop order unchanged) | ✅ builds as a static lib | — |
| 2 tvOS cross-compile | ✅ unaffected | ✅ static lib builds (winit 0.30 patched, SDL3 video on for tvOS) | ✅ device static lib (`aarch64-apple-tvos`) builds, and the unsigned device-SDK Xcode build links; signing + install need the team ID and the Apple TV |
| 3 App shell (Xcode/XcodeGen, `scripts/build-tvos.sh`) | — | ✅ builds, installs, launches on "Apple TV 4K (4th generation)" Simulator, tvOS 27.2 | 📺 |
| 3 Engine runs | ✅ | ✅ **front end renders** (main menu + profile prompt) and **a race runs** (`alps 993 --opponents 3`: in-car view, HUD, mirror, race clock) from your disc's data copied into the Simulator app's Caches/game; audio output opens (CoreAudio 48 kHz stereo); SDL joystick layer starts | 📺 |
| 3 "Game data missing" gate | — | 🟡 Stage-3 placeholder screen written (shown when Caches/game is empty); not yet screenshotted | 📺 |

### tvOS fixes found in the Simulator (all in TVOS.md)
1. tvOS 27 SDK **requires the UIScene life cycle** (crash `UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`): scene manifest + Swift `SceneDelegate`; winit's window attaches to the scene; the engine waits for `ou_scene_ready()`.
2. winit called **iOS-only UIKit selectors** (`setNeedsUpdateOfHomeIndicatorAutoHidden`, status bar, rotation, multi-touch) → skipped on tvOS in `third_party/winit`.
3. SDL on UIKit needs `SDL_SetMainReady()` (no `SDL_main`).
4. Simulator GPU offers 15 inter-stage shader variables (WebGPU default asks 16) → the request is capped to the adapter.
5. winit's UIKit backend drops redraws requested inside a frame → the tvOS handler requests the next redraw from `about_to_wait` (the engine drew only one frame before this).
6. winit logs every frame on UIKit → `winit=error` log filter on tvOS.

Known gaps (next steps): the FE **mouse pointer is visible** on tvOS (hide it, enhancement 2.6); no app icon yet (placeholder); the
"start-up movies" are skipped (no `nfs5.exe` on the disc — see above).

## OpenUnleashed server (`tools/ou-server`, `docs/ou-server.md`)
| Part | State |
|---|---|
| `crates/ou-manifest` (shared) + `tools/make-manifest` | ✅ your data: 1,593 files, 528.8 MB, ~2 s; the server's manifest is byte-identical |
| Phase 1 — info + data sets (`original`, `mod-<name>` overlays, `hd`), manifests with hash cache, files with Range/If-Range/ETag | ✅ tested on the Mac: resumed download of the largest file (117.6 MB) verifies against its SHA-256; 416 past end; path escapes 400; files outside the manifest 404 |
| Phase 2 — saves (`If-Match`/412, versions: last N + one per day for 30 days, restore) | ✅ server side; tested (Mac/TV conflict keeps both, restore works). Client side: ⬜ not started (Stage 9 item 7) |
| Phase 3 — lobby (register/heartbeat/expiry, address order for tailnet vs LAN, `?via=`) | ✅ server side; tested. Client side: ⬜ not started |
| Status page (`/?token=…`): sets, saves + versions with download/restore, games, devices | ✅ |
| Auth (Bearer / `?token=`), PUID/PGID drop, `--healthcheck` | ✅ |
| Tests | ✅ `cargo test -p ou-manifest -p ou-server`: 11 passed |
| Docker image (`tools/ou-server/Dockerfile`, context = repo root) | ✅ built and run locally (Colima, arm64): healthy, runs as UID 99, real game data mounted read-only, mod overlay + save verified |
| GitHub Actions → `ghcr.io/dannyelson82/ou-server` (`.github/workflows/ou-server-image.yml`, same pattern/pinned actions as ReelHaven) | 🟡 written, YAML-checked; **not run** — see Blockers |
| Unraid template `packaging/unraid/ou-server.xml` (all under `/mnt/user/appdata/ou-server`) | ✅ written, XML-checked; you add the container yourself |
| Compose file, Bonjour (avahi service file), Tailscale (a/b, `tailscale serve` HTTPS) docs | ✅ |
| Phase 4 (dedicated host, EA lobby revival) | ⬜ research only, not started |

Local tools installed for this: Colima + Docker CLI + buildx (Homebrew, user-local; `colima start` / `colima stop`).

## Needs Apple TV
- Stage 3: install + launch on the device; check winit/UIKit behaviour on real hardware.
- Signing (also needs `<MY_APPLE_ID_OR_TEAM>`): device builds, `scripts/build-tvos.sh device`.

## Blockers
- **Publishing the image to GHCR needs a GitHub repo of yours.** This clone's `origin` is upstream
  (`Second-Boot/Open-Unleashed`). The workflow runs once the branch is pushed to e.g. `github.com/dannyelson82/Open-Unleashed`
  (a fork); the Unraid template's icon/template URLs assume that name. Creating the fork and pushing is outward-facing, so
  it's waiting for your OK (and `gh`/credentials — no GitHub login is set up on this Mac).
