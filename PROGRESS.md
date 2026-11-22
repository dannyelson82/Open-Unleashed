# PROGRESS — OpenUnleashed macOS + tvOS (`tvos-port` branch)

Status of every stage of `openunleashed-tvos-prompt.md` (+ `enhancements-plan.md`, `airpods-plan.md`,
`server-plan.md`, `hd-texture-plan.md`) on both platforms. Details of every change: `TVOS.md`.

Legend: ✅ done & verified · 🟡 partly / done but unverified · ⛔ blocked · 📺 **needs Apple TV** (real device, from Wednesday) · — not applicable

_Last updated: 2026-11-21 (session paused: usage limit)_

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

## Decisions to review
Choices made without asking (per your instruction); each is easy to change.
1. **winit**: patched 0.30 (`third_party/winit`) instead of upgrading to the 0.31 beta — smallest change, macOS untouched.
2. **SDL3 on tvOS**: video subsystem compiled in (never initialised) because SDL's UIKit code needs it to link.
3. **Engine split** into `lib.rs` + thin `main.rs` so tvOS links the same code as a static library.
4. **Disc**: `Porsche.exe` (SafeDisc) not copied as `nfs5.exe` → intro movies skipped.
5. **Fork name** `dannyelson82/Open-Unleashed` (public, like every fork of a public repo); Unraid template URLs point at its `tvos-port` branch.
6. **Image tags**: `latest` + `edge` are published from pushes to `tvos-port` (and `main`), `X.Y.Z` from `ou-server-vX.Y.Z` tags, plus the short SHA — so the Unraid template's `:latest` works without a release.
7. **Save API paths** use `/files/`, `/versions/`, `/version/{id}/`, `/restore/{id}/` segments before the file path (the plan's shorter paths clash with file paths containing `/`).
8. **Lobby** accepts `?via=tailnet|lan` from the client, because behind Docker NAT the server may not see the device's real address.
9. **Stand-in bundle ID** `org.openunleashed.tvos` until `<com.yourname.openunleashed>` is given; Simulator default "Apple TV 4K (4th generation)".
10. **Local Docker**: Colima (Homebrew, no admin rights) instead of Docker Desktop.
11. **Game Data screen in UIKit** (app shell, Swift) rather than drawn by the engine — the engine can't start without the data; focus engine gives Siri Remote + controller navigation for free.
12. **Launch check = sizes + a "verified" marker** of the last complete download (no hashing ~0.5 GB per launch); full SHA-256 only while downloading.
13. **ATS**: `NSAllowsLocalNetworking` + a narrow `ts.net` exception (plain HTTP inside WireGuard) so the tailnet name works even without `tailscale serve` HTTPS; HTTPS is still tried first for `*.ts.net`.
14. **Save mirror by polling** (every 15 s + on background, size/mtime stamp) instead of a hook in the engine's save code — no engine change; worst case a save made < 15 s before tvOS purges Caches *and* kills the app is lost locally (the server sync will cover it).
15. **Default server address** `tower.local`; token must be entered once.
16. **tvOS command line**: the engine ignores `-ou.*` UserDefaults launch arguments (they're the shell's).
17. **B / Menu = back on tvOS only**: the original's front end reads no pad; the Mac keeps that (authentic), tvOS needs a back button.
18. **Siri Remote Menu never leaves the app** (Apple's guideline wants Menu at the top level to go Home; for a sideloaded personal build the game's own "back"/exit question is more useful). Home (TV button) still leaves.
19. **On-screen keyboard opens by itself** when a text field becomes active (once per field); a chat box only on Y, since it is always focused.
21. **Render resolution via the view's content scale** (system upscales) instead of an engine render target — default native until measured on the device.
26. **Head-look**: bumper/driver views only (the plan's "also in chase views" left out for now); recentre = race start, unpause, pad L3+R3 (no keyboard key bound); positive head yaw = view left — to confirm with AirPods.
24. **HUD size by a smaller "virtual screen"** for the HUD's layout: every element scales, the original's anchoring rules run unchanged; default Original on both platforms for now (the plan suggests Large on tvOS — decide on the TV).
25. **Extras opened from the pause menu first** (the race HUD's text drawing exists there); the main-menu entry follows.
23. **Unsigned .ipa** for atvloadly (it signs with its own Apple ID) — nothing needs your Apple ID on the Mac.
22. **Siri Remote not counted as a racing pad** (matched by name "remote").
20. **Pause on controller loss also on the Mac** (the shared `request_pause`; the keyboard could keep racing, but a pad vanishing mid-corner is worth a pause).

## Stage status
| Stage | macOS | tvOS Simulator | Apple TV |
|---|---|---|---|
| 1 Baseline build | ✅ release builds; runs; without data exits with the clear "game data not found" error | — | — |
| 1 Race starts | ✅ `NFS5_GAME_PATH=game-data open-unleashed alps 993`: 20 s headless race (car accelerates, shifts 1→4); windowed `--screenshot` race with 3 opponents renders correctly on Metal (Apple M2 Pro) | — | — |
| 1 Engine as a library | ✅ `crates/open-unleashed` is now `lib.rs` + a 3-line `main.rs`; `run()` split into `build_app()` + event loop (desktop order unchanged) | ✅ builds as a static lib | — |
| 2 tvOS cross-compile | ✅ unaffected | ✅ static lib builds (winit 0.30 patched, SDL3 video on for tvOS) | ✅ device static lib (`aarch64-apple-tvos`) builds, and the unsigned device-SDK Xcode build links; signing + install need the team ID and the Apple TV |
| 3 App shell (Xcode/XcodeGen, `scripts/build-tvos.sh`) | — | ✅ builds, installs, launches on "Apple TV 4K (4th generation)" Simulator, tvOS 27.2 | 📺 |
| 3 Engine runs | ✅ | ✅ **front end renders** (main menu + profile prompt) and **a race runs** (`alps 993 --opponents 3`: in-car view, HUD, mirror, race clock) from your disc's data copied into the Simulator app's Caches/game; audio output opens (CoreAudio 48 kHz stereo); SDL joystick layer starts | 📺 |
| 4 Game Data screen (Swift: `GameDataViewController`, `GameStore`, `ServerClient`, `Bonjour`) | — | ✅ fresh install → screen → downloads all 1,593 files (529 MB) from ou-server at ~79 MB/s with overall/per-file progress, SHA-256 per file, then the engine starts (main menu). Killed mid-download (306 MB + 76.5 MB partial) → relaunch completes and verifies. Server/token in UserDefaults, Find on network (Bonjour), Test connection, data-set choice (original/mod sets), clear errors (unreachable / token / disk full / hash mismatch) | 📺 LAN + Tailscale download from tower |
| 4 Launch check | — | ✅ verified manifest + per-file size check (no hashing on launch) | 📺 |
| 4 Caches purge → re-download only missing files | — | ⛔ not run: deleting part of the Simulator's Caches was blocked by a safety check (variable path); the code path is the same size check as the launch check. To try: delete a few files under the app's `Library/Caches/game/GameData/Music/` and relaunch | 📺 |
| 4 Save mirror to NSUserDefaults (`SaveMirror.swift`) | — | 🟡 written (every 15 s + on background, ≤ 400 KB, restore-missing before the engine starts); not exercised yet — no career save exists yet | 📺 |
| 4 Save sizes | 🟡 estimate from the format: `.sav` ≈ 8 KB missions + 0.36 KB/market part + ~1.7 KB/car ≈ 20–80 KB per profile; `nfs5.trk` 17.9 KB → several profiles fit 400 KB. Exact figure: the mirror logs it on the first real save | — | — |

| 5 Siri Remote | — | 🟡 SDL already exposes it as a pad (click = A, Menu = B, Play/Pause = X, touch surface = left stick); B / Menu = the front end's Esc on tvOS; Menu no longer sends the app to the Home screen (winit view controller swallows the press). Not testable without a remote | 📺 |
| 5 Text entry (profile name, address, replay name, input dialogs, chat) | ✅ Mac unchanged (Return now goes through one shared function) | ✅ native tvOS keyboard prompt opens by itself on "create player" ("Player name", up to 8 characters); A (Y on a chat box) reopens it. Typing + OK needs a person | 📺 |
| 5 Pointer hidden | — | ✅ parked outside the screen: no pointer, no hover tooltips | 📺 |
| 5 Pause when the last controller disconnects mid-race | ✅ (shared `request_pause`) | 🟡 code path shared with the background pause (verified); hot-unplug not possible in the Simulator | 📺 |
| 5 "Connect a controller" notice | — | 🟡 drawn in a race when no racing pad is connected (Siri Remote excluded); not visible in the Simulator, which provides a virtual "Gamepad" | 📺 |
| 6.1 AVAudioSession (playback, multichannel declared) | — | ✅ set before the engine opens audio (Simulator: Speaker, 2/2 channels) | 📺 channel count with your TV/receiver/AirPods |
| 6.2 Intro movies | ✅ skipped cleanly (no `nfs5.exe`) | ✅ same | — |
| 7.1 Frame-time overlay (`NFS_FPS_OVERLAY=1`, tvOS `-ou.fpsOverlay YES`) | ✅ | ✅ shows fps / avg / worst / budget (Simulator 60.0 fps — not representative) | 📺 real numbers |
| 7.2 60 Hz display criteria | — | 🟡 requested on scene activation (only acts with Match Frame Rate on) | 📺 |
| 7.3 Render resolution (`-ou.renderScale 1/1.5/2`) | — | ✅ surface 1760×960 / 2640×1440 / 3520×1920. Note: the window is 1760×960 points, not 1920×1080 — a TV safe-area inset in winit's window? To check (safe-area enhancement) | 📺 pick the default by measuring |
| 7.4–7.5 Shader stutter, Instruments profiling | — | — | 📺 |
| 8 `.ipa` for atvloadly (`scripts/export-ipa.sh`) | — | ✅ unsigned device build → `target/tvos/OpenUnleashed.ipa` (5.5 MB, checked: no game files) | 📺 install via atvloadly; check refresh keeps Caches + UserDefaults |
| 8 atvloadly docs (TVOS.md) | ✅ | — | — |

### Stage 9 — enhancements
| Item | macOS | tvOS Simulator | Apple TV |
|---|---|---|---|
| 0 Extras menu + `extras.json` (`nfs_game::extras`, `app/extras_ui.rs`) | ✅ pause menu's last item "extras"; pages/rows with value, explanation, "applies now/at next start", reset page; arrows/d-pad; saved + reloaded (tests); `extras.json` is in the tvOS save mirror | ✅ (same code; settings file read at start) | 📺 pad/remote navigation |
| 0 Extras from the main menu (pad View / F12 / Siri Remote Play/Pause) | ⬜ the front end has no HUD font object yet — needs its own text path | ⬜ | 📺 |
| 2.2 HUD size Original / Large (1.5×) / Extra large (2×) | ✅ (pointer hover follows) | ✅ screenshots: every element scaled, original edge anchoring kept, nothing clipped | 📺 couch check on a 55″ TV |
| 7.1 Frame-time overlay switch in Extras | ✅ | ✅ | — |
| 6b-H Mac head-look (AirPods) | ✅ built in; filter unit-tested (5 tests: dead zone, 20°→75°, clamp, jump guard, look-key wins, drift); inside the app bundle CoreMotion starts ("motion updates started"); the bare binary refuses safely. 🙋 **needs you with AirPods**: direction, feel, 10-minute no-drift check | — (hidden: no API) | — |
| 7.3 Render resolution in Extras (tvOS only) | — | ✅ row shown on tvOS only; read at window creation | 📺 |
| 5 Xbox/PlayStation pads via SDL's GameController driver, rumble | — | — | 📺 (no pads in the Simulator) |
| 6.3 Lifecycle | — | ✅ Home/other app mid-race → race pauses ("app in background"), audio stream stops; back → audio restarts, pause menu shown, no crash; saves mirrored on background | 📺 |

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
| GitHub Actions → `ghcr.io/dannyelson82/ou-server` (`.github/workflows/ou-server-image.yml`, same pattern/pinned actions as ReelHaven) | ✅ **published**: fork `github.com/dannyelson82/Open-Unleashed` (branch `tvos-port`), run 44932561662 green (build, smoke test, Trivy, publish); tags `latest`, `edge`, `sha-5a34ce2`; package **public** (Unraid pulls without login). First run failed only on a test bug (process-name match) — fixed |
| Unraid template `packaging/unraid/ou-server.xml` (all under `/mnt/user/appdata/ou-server`) | ✅ written, XML-checked; you add the container yourself |
| Compose file, Bonjour (avahi service file), Tailscale (a/b, `tailscale serve` HTTPS) docs | ✅ |
| Phase 4 (dedicated host, EA lobby revival) | ⬜ research only, not started |

Local tools installed for this: Colima + Docker CLI + buildx (Homebrew, user-local; `colima start` / `colima stop`).

## Next (in order, not started)
1. Stage 9 item 0: Extras settings menu + `extras.json` (switches for the overlay, render scale, etc.).
3. Server clients in the engine: save sync (phase 2) and lobby (phase 3).
4. Remaining enhancements 1–11, the rest of the AirPods plan (route changes, auto-pause on disconnect), LAN multiplayer test.

## Needs Apple TV
- Stage 3: install + launch on the device; check winit/UIKit behaviour on real hardware.
- Signing (also needs `<MY_APPLE_ID_OR_TEAM>`): device builds, `scripts/build-tvos.sh device`.

## Blockers
- (resolved) GHCR publishing — fork created and image published after your OK.
