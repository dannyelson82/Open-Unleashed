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
OU_TEAM_ID=XXXXXXXXXX OU_BUNDLE_ID=com.you.openunleashed scripts/build-tvos.sh device
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
1. the Swift shell's `main` sets `NFS5_GAME_PATH=Caches/game`, `NFS5_DATA_DIR=Caches/save` and calls
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
| saves / settings / log (`data_dir()`) | per-OS app data folder | `NFS5_DATA_DIR`, else `Library/Caches/save` |
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

## Sideloading with atvloadly (Stage 8)
[atvloadly](https://github.com/bitxeno/atvloadly) installs and keeps refreshing sideloaded apps on an Apple TV. It
runs in Docker on Linux — here on the Unraid server.

**Build the .ipa** (on the Mac; unsigned — atvloadly signs it with its own Apple ID while installing):
```sh
scripts/export-ipa.sh                       # → target/tvos/OpenUnleashed.ipa (~5.5 MB, no game files)
OU_BUNDLE_ID=com.you.openunleashed scripts/export-ipa.sh
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
3. Upload `OpenUnleashed.ipa`, install, and turn on automatic refresh.

**Limits and gotchas:**
- A free Apple ID: apps are signed for **7 days** (atvloadly refreshes them); up to **10 app IDs registered**, **3
  apps active** at once.
- **Re-pair after every tvOS update**, and brand-new tvOS versions are often unsupported at first — **turn off
  automatic tvOS updates** (Settings › System › Software Updates).
- atvloadly **changes the bundle ID** when it signs, which would break CloudKit — irrelevant here: saves sync through
  the OpenUnleashed server, never iCloud.

**What survives a refresh / reinstall** (to confirm on the device): a refresh re-signs and reinstalls the same app,
which keeps its data container — the downloaded game data in `Library/Caches/game` (unless tvOS purged it) and the
`NSUserDefaults` save mirror + server setting. Deleting the app from the Apple TV deletes both.

## Known issues
(see PROGRESS.md — kept current there)
