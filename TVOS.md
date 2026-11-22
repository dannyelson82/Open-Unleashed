# OpenUnleashed on Apple TV (tvOS) — port notes

Personal-use prototype of the engine on Apple TV 4K (tvOS, aarch64), built from the same codebase as the
macOS app. Branch `tvos-port`. Licence: GPL-3.0-or-later, like the rest of the project (`LICENSE`); third-party
licences unchanged. **No file of the original game is ever committed, bundled into the app or uploaded** —
`.gitignore` blocks `game-data/`, `game/`, `GameData/`, `FEData/`, disc images and `nfs5.exe`.

Status per platform: `PROGRESS.md`.

## Toolchain
- Rust ≥ 1.95 (edition 2024) via rustup; tested with 1.100.0.
- `rustup target add aarch64-apple-tvos aarch64-apple-tvos-sim` — tier 2 on stable, no nightly / `-Zbuild-std`.
- CMake (SDL3 is built from source by `sdl3-sys`), Xcode with the tvOS SDK, XcodeGen (`brew install xcodegen`).

## Build commands
```sh
# macOS (unchanged)
cargo build --release
NFS5_GAME_PATH=$PWD/game-data cargo run --release -p open-unleashed

# tvOS: engine static library only
CARGO_TARGET_DIR=target/tvos cargo rustc --release --target aarch64-apple-tvos-sim -p open-unleashed --lib --crate-type staticlib
CARGO_TARGET_DIR=target/tvos cargo rustc --release --target aarch64-apple-tvos     -p open-unleashed --lib --crate-type staticlib

# tvOS: everything (cargo → XcodeGen → xcodebuild → install + launch)
scripts/build-tvos.sh sim            # Simulator
scripts/build-tvos.sh sim --data     # Simulator, copying ./game-data into the app's Caches/game (testing only)
NFSH_TEAM_ID=XXXXXXXXXX scripts/build-tvos.sh device
```
The tvOS build uses its own target dir (`target/tvos`) so it can run in parallel with the macOS build.

## Architecture of the port

### Engine = library, desktop binary = 3 lines
`crates/open-unleashed/src/main.rs` became `lib.rs` (crate `open_unleashed`); `main.rs` just calls
`open_unleashed::main()`. The tvOS app links the same library as a static library
(`cargo rustc --crate-type staticlib`; the crate type is chosen on the command line so the desktop build doesn't
compile a second, fat-LTO static library). `run()` was split into `build_app()` (everything the launch sets up
before the event loop: data, command line, profile, first track/car → `App`) and the event loop. On the desktop the
order of the steps is unchanged (the event loop is still made at the same point via a callback).

### tvOS start-up (`crates/open-unleashed/src/tvos.rs`)
winit's UIKit backend owns the process (`UIApplicationMain`, never returns), and on tvOS the game data is only
downloaded *after* UIKit is up. So on tvOS:
1. the Swift shell's `main` sets `NFS5_GAME_PATH=Caches/games/porsche/data`, `NFS5_DATA_DIR=Caches/games/porsche/save` and calls
   `ou_main(&callbacks)`;
2. the event loop starts at once with a `Boot` handler that needs no game data;
3. on the first `resumed` the shell's `launched` callback checks the data and shows its own screen (a separate
   `UIWindow`), or calls `ou_data_ready()`;
4. `ou_data_ready()` → `Boot` runs `build_app()` and forwards every event to the `App` from then on. If the
   engine refuses the data, `engine_error(msg)` puts the shell's screen back with the message.

C ABI (`platform/tvos/OpenUnleashed/OpenUnleashed-Bridging-Header.h`): `ou_main`, `ou_data_ready`, `ou_request_quit`.

Why not winit's own app start alone: the Game Data screen must run before any game file exists, and the engine's
start needs them. Why not a Swift-owned `UIApplicationMain`: winit 0.30 must call `UIApplicationMain` itself.

### Paths on tvOS (`crates/nfs-assets/src/platform.rs`)
| What | Desktop | tvOS |
|---|---|---|
| game data | `NFS5_GAME_PATH`, `game/`, … | `NFS5_GAME_PATH` = `Library/Caches/game` (also a fallback candidate) |
| saves / settings / log (`data_dir()`) | per-OS app data folder | `NFS5_DATA_DIR`, else `Library/Caches/games/porsche/save` |
| photo mode stills | `~/Pictures/PorscheUnleashed` | `Library/Caches/photos` |

### Platform branches added
| File | Desktop | tvOS |
|---|---|---|
| `nfs-assets/src/platform.rs` | unchanged | Caches paths (above) |
| `open-unleashed/src/netlobby.rs` (REGISTERUSER opens a web page) | `open` / `cmd start` / `xdg-open` | logged only (no browser, no `std::process::Command`) |
| `open-unleashed/src/diag.rs` (fatal error box) | CF alert / MessageBox / zenity | log only (the shell shows `engine_error`) |
| `open-unleashed/src/netplay/keys.rs` (OS Escape state) | CGEventSource / GetAsyncKeyState | falls into the "no global key state" branch (false) — no change needed |
| `open-unleashed/build.rs` (`embed-resource`) | Windows only | already a no-op (checks `CARGO_CFG_TARGET_OS == windows`) |

## Patched / reconfigured crates
| Crate | Change | Why |
|---|---|---|
| **winit 0.30.13** → `third_party/winit` (`[patch.crates-io]`) | `build.rs`: `ios_platform` and `apple` cfg aliases also for `target_os = "tvos"`; `Cargo.toml`: every `target_os = "ios"` dependency table also for tvOS (and tvOS excluded from the X11/Wayland deps) | 0.30 maps only iOS to the UIKit backend, so tvOS fell through to X11/Wayland |
| **sdl3-sys 0.7** (SDL 3.4.18) | for tvOS only, the dependency is declared without `no-sdl-video` and `sdl-unix-console-build` (`crates/nfs-input/Cargo.toml`) | with SDL_VIDEO=OFF, SDL's UIKit code still references its UIKit video driver (`SDL_IsAppleTV`, `SDL_UpdateLifecycleObserver`) and the shared-library link fails. The video subsystem is compiled in but never initialised (only gamepad/haptic) |
| wgpu 30 / Metal | none | tvOS supported as is |
| cpal 0.18 / coreaudio-rs 0.14 | none | tvOS supported as is |

### winit: 0.31 upgrade vs patched 0.30 — decision
- (a) winit 0.31 (`winit-uikit`, beta) supports tvOS, but the API changed (application handler, window creation,
  event types) across ~55 engine call sites, and it's still a beta — a macOS regression risk for the whole game.
- (b) patching 0.30: two cfg aliases + the dependency tables; the UIKit backend's code is shared with iOS. Risk:
  iOS-only UIKit calls at runtime (status bar, orientation, multi-touch) — checked in the Simulator.
- **Chosen: (b)**, smallest change, macOS untouched. Revisit when 0.31 is stable.

## Game data updates

At launch, with a complete verified download and a server set, the shell (`Shell.updateAvailable`) fetches the server's
manifest for the chosen set and compares it with `Caches/game/.ou/manifest.json` (`GameStore.differences`): files whose
SHA-256 differs or that are new, and files no longer listed. It waits at most ~4 s; offline or slow, the game starts with
what's there. If anything differs, the Game Data screen says how many files / MB changed and offers **Update** (the normal
download, which keeps every file whose hash already matches, so only the changed files are fetched) or **Play**.
Tested in the Simulator: 3.2 → 3.5 patch (7 files, 1 MB), then `nfs5.exe` added (1 file, 2 MB); the next launch went
straight into the game.

## Sideloading with atvloadly (Stage 8)
[atvloadly](https://github.com/bitxeno/atvloadly) installs and keeps refreshing sideloaded apps on an Apple TV. It
runs in Docker on Linux — here on the Unraid server.

**Build the .ipa** (on the Mac; unsigned — atvloadly signs it with its own Apple ID while installing):
```sh
scripts/export-ipa.sh                       # → target/tvos/NFSHaven.ipa (~5.5 MB, no game files)
NFSH_BUNDLE_ID=com.dannyelson.nfshaven scripts/export-ipa.sh   # (the default)
```

**Install atvloadly on Unraid** (Docker › Add Container, or compose):
- image `ghcr.io/bitxeno/atvloadly:latest`, port **`5533:80`** (web UI at `http://tower.local:5533`), appdata
  `/mnt/user/appdata/atvloadly` → `/data`;
- it needs the host's **avahi-daemon and D-Bus**: mount `/var/run/dbus:/var/run/dbus` and
  `/var/run/avahi-daemon:/var/run/avahi-daemon` (Unraid already runs avahi); add
  `--security-opt apparmor=unconfined` (or run privileged) if pairing can't see the Apple TV;
- network: host or bridge as atvloadly's README says for your setup.

**Pair and install:**
1. Apple TV: Settings › Remotes and Devices › Remote App and Devices — leave it open; pair from atvloadly's web UI
   (enter the PIN shown on the TV).
2. Add an Apple ID in atvloadly — use a **dedicated Apple ID** and keep its 2FA device at hand (the code is asked
   for); **app-specific passwords are not supported**.
3. Upload `NFSHaven.ipa`, install, and turn on automatic refresh.

**Limits and gotchas:**
- A free Apple ID: apps are signed for **7 days** (atvloadly refreshes them); up to **10 app IDs registered**, **3
  apps active** at once.
- **Re-pair after every tvOS update**, and brand-new tvOS versions are often unsupported at first — **turn off
  automatic tvOS updates** (Settings › System › Software Updates).
- atvloadly **changes the bundle ID** when it signs, which would break CloudKit — irrelevant here: saves sync through
  the NFS Haven server, never iCloud.

**What survives a refresh / reinstall** (to confirm on the device): a refresh re-signs and reinstalls the same app,
which keeps its data container — the downloaded game data in `Library/Caches/game` (unless tvOS purged it) and the
`NSUserDefaults` save mirror + server setting. Deleting the app from the Apple TV deletes both.

## Port features (both platforms unless noted)
All off / original by default; switched in **Extras** (pause menu › extras), saved in `extras.json` beside the saves
(`nfs_game::extras`; mirrored on tvOS, synced through the server).
| Feature | Where | Code |
|---|---|---|
| Frame-time overlay | Display & HUD | `app/overlay.rs` (`NFS_FPS_OVERLAY=1`) |
| HUD size Original / Large / Extra large | Display & HUD | `hud/geometry.rs` (`Hud::set_scale`) |
| Render resolution Native / 1440p / 1080p (tvOS) | Display & HUD | `tvos.rs::apply_render_scale` |
| Anisotropic filtering, draw distance (+fog), FOV offsets | Graphics | `nfs-renderer` (`set_anisotropy`), `race_stages.rs`, `render_scene.rs` |
| Anti-aliasing: Off / FXAA / MSAA 4× (next start) | Graphics | `nfs-renderer`: `FxaaPass`; MSAA = the 66 mesh pipelines built a second time at 4 samples, the 3-D pass drawn into multisampled targets and resolved into the frame (not for a front-end frame whose sprites test the 3-D depth; no alpha-to-coverage) |
| HD textures: Original / HD (next race) | Graphics | `nfs_assets::hd` (see "HD texture packs") |
| Steering curve, speed-sensitive steering, countersteer, ABS | Driving assists | `nfs_game::assists` (applied to the local packet: replays/network exact) |
| Pause when headphones disconnect; output follows the default device | Audio | `nfs-audio` `route_check`, `app/lifecycle.rs` |
| Head-look with AirPods (Mac) | Controller | `nfs_game::headlook`, `app/headlook.rs` |
| Button names follow the last device (Xbox / PlayStation / …) | automatic | `nfs_input::glyphs` |
| Save sync + lobby through nfs-haven-server | Network (server, password, test; Mac: sync saves) — kept per device in `network.json`; on tvOS also the Game Data screen's Server, the same setting (the shell's callback `server_changed`) | `sync.rs`, `server_lobby.rs`, `extras_ui.rs` |

tvOS-only: Game Data screen (download from nfs-haven-server), on-screen keyboard for text fields, B / Menu = back, Siri
Remote Menu never leaves the app, pointer hidden, pause on background, no screen saver during races, layered icon +
Top Shelf, save mirror in NSUserDefaults.

## HD texture packs (Stage 1: engine side)

`nfs_assets::hd` — an optional pack of higher-resolution copies of the game's own texture entries, made from your
own copy and never shared. **Extras › Graphics › Textures: Original / HD** (from the next race); the frame overlay
then also shows "HD textures in use: x / y".

* **Pack** = `index.json` (`{format: 1, scale: 2|4, entries: {key: {file, category, source}}}`) + one KTX2 per entry
  (RGBA8, or ASTC 4×4 — uploaded as blocks where the GPU samples ASTC: every Apple Silicon Mac and Apple TV; an
  Intel Mac falls back to the original). Looked for at `NFS_HD_PATH`, else `hd/` in the data folder (Mac:
  `~/Library/Application Support/PorscheUnleashed/hd`; tvOS: `Caches/games/porsche/hd`, the server's `porsche/hd`).
* **Key** = the first 16 bytes (hex) of the SHA-256 of the entry's raw bytes (`Fsh::entry_bytes`), the same in
  `tools/texture-pack` and the engine. An HD texture keeps the entry's logical size (layout unchanged), must be
  exactly scale × it, and skips the original's 16-bit cache reduction.
* **Routed now**: track textures and their `.env` maps, sky, `common.fsh`, `particle.fsh` (effects, lights, shadows),
  the front-end room's textures. **Not yet**: car pages (entries blitted at pixel positions — the plan does cars
  last; car packs bypass the store), front-end menu art.
* `NFS_HD_DUMP=1` appends every entry uploaded through the store to `hd-used.csv` in the data folder.
* Stand-in pack for testing: `texture-pack fake <game folder> --out DIR [--scale 2] [--match alps] [--only track,sky]
  [--astc]` (nearest-neighbour; `--astc` uses ARM's `astcenc`, e.g. `target/tools/astcenc/bin/astcenc`).

## Apple TV settings that matter
- **Match Frame Rate / Match Dynamic Range** (Settings › Video and Audio › Match Content): the app asks for 60 Hz SDR;
  with Match Dynamic Range on, a future HDR mode will switch the TV to HDR.
- **Audio Format**: Auto sends multichannel PCM over HDMI (for a future 5.1 mode); through a TV's ARC usually only
  stereo arrives (eARC passes 5.1 PCM).
- **Automatic tvOS updates off** (atvloadly must be re-paired after updates).

## AirPods
- **Mac head-look** (`airpods-plan.md` H; macOS 14+; Extras › Controller › Head-look, off by default): AirPods
  head tracking turns the bumper / driver view (strength Low/Medium/High = 50/75/100° of view for 20° of head
  turn, optional up/down tilt ±15°). Dead zone 3°, smoothing ~80 ms, max 110°, drift correction ~1°/s near
  centre, recentre on a >30° jump, at race start, after a pause and on pad L3 + R3; a held look key wins.
  Render-only — replays and network races are unaffected. Code: `nfs_game::headlook` (pure, tested),
  `open-unleashed/src/app/headlook.rs` (CoreMotion, macOS only). Needs the **app bundle**
  (`packaging/make_app.sh` adds `NSMotionUsageDescription`); a bare `cargo run` binary logs why and leaves it off.
  macOS asks for Motion & Fitness permission on first use; if denied it stays off.
- **Not on tvOS**: `CMHeadphoneMotionManager` doesn't exist on tvOS, so the rows are hidden there.
- Apple TV: the app declares multichannel content (`setSupportsMultichannelContent`) — spatial audio with
  AirPods is up to tvOS (probe P1 on the device).

## Known issues
(see PROGRESS.md — kept current there)
