# PROGRESS — OpenUnleashed macOS + tvOS (`tvos-port` branch)

Status of every stage of `openunleashed-tvos-prompt.md` (+ `enhancements-plan.md`, `airpods-plan.md`,
`server-plan.md`, `hd-texture-plan.md`) on both platforms. Details of every change: `TVOS.md`.

Legend: ✅ done & verified · 🟡 partly / done but unverified · ⛔ blocked · 📺 **needs Apple TV** (real device, from Wednesday) · — not applicable

_Last updated: 2026-11-21 (session paused: usage limit)_

## Morning summary
- **Works on the Mac and in the tvOS Simulator** from your disc's data: front end, races, Game Data download from
  nfs-haven-server (verified, resumable), save sync + multiplayer lobby through nfs-haven-server, Extras menu (HUD size, graphics,
  driving assists, FXAA, audio route handling, Mac head-look), lifecycle (pause on Home), on-screen keyboard, app icon + Top Shelf.
- **nfs-haven-server** is published (`ghcr.io/dannyelson82/nfs-haven-server:latest`, public); add it on Unraid with
  `packaging/unraid/nfs-haven-server.xml` (data under `/mnt/user/appdata/nfs-haven-server`).
- **Needs you**: the Apple TV (everything marked 📺), a pad for feel/rumble, AirPods for head-look and route tests (🙋),
  and the "Decisions to review" below.
- **Not started / partly**: split-screen, HDR,
  HD-pack engine hooks, mod sets (no mods to test with), full hint tokens + SF Symbols glyphs, ~~Mac UI for the server~~ (done: Extras › Network)
  address, MSAA.

## NFS Haven rename (2026-11-22)
Clean rename, as asked (nothing was installed anywhere yet, so no compatibility names are kept):
- **Server**: `ou-server` → **`nfs-haven-server`** (crate/binary `tools/nfs-haven-server`, version 0.2.0; shared
  crate `crates/nfs-haven-manifest`), image **`ghcr.io/dannyelson82/nfs-haven-server`** (workflow
  `nfs-haven-server-image.yml`; tags `latest`/`edge`, `nfs-haven-server-vX.Y.Z`, short SHA), Unraid template
  **`packaging/unraid/nfs-haven-server.xml`** (container `nfs-haven-server`, appdata
  `/mnt/user/appdata/nfs-haven-server/{games,data}`), env **`NFSH_*`**, header `X-NFSH-Device`, Bonjour
  **`_nfs-haven._tcp`**, status page "NFS Haven server". The old `ou-server` workflow is gone, so nothing publishes
  `ghcr.io/dannyelson82/ou-server` any more (the package and its last images still exist on GitHub; delete it under
  Packages when you like — not done here, it can't be undone).
- **Games layout** (`nfs-haven-plan.md` Phase 5): `/games/<id>/original`, `/games/<id>/mods/<name>`, `/games/<id>/hd`;
  set ids `porsche/original`, `porsche/mod-<name>`, `porsche/hd`; API `/v1/data/{game}/{set}/…`; `/v1/info` lists
  `games`. Porsche lives in `games/porsche/original`. A new game = a new folder (served whole until it has rules).
  Saves: one namespace per game (`porsche`).
- **App**: display name **NFS Haven**, bundle ID **`com.dannyelson.nfshaven`** (tvOS and the Mac app), settings keys
  and launch arguments `nfsh.*` / `-nfsh.*`, data in `Caches/games/porsche/{data,save,photos}`, build variables
  `NFSH_BUNDLE_ID` / `NFSH_TEAM_ID` / `NFSH_SIM_NAME`, `.ipa` → `target/tvos/NFSHaven.ipa`.
- **Checked**: server tests (8, incl. a second game folder served whole; manifest crate 4), local `nfs-haven-server` on the games
  layout (530 MB `porsche/original`), and in the Simulator the old app removed, **NFS Haven** installed fresh:
  full download from the new server into `Caches/games/porsche/data`, intro movie plays, save sync lands under
  `porsche` with the device name. CI smoke test covers two games, mod overlay, saves, PUID 99.

## Test suite (2026-11-21)
`NFS5_GAME_PATH=game-data cargo test --release --workspace --no-fail-fast`: **1,394 passed, 13 failed, 7 ignored.** The 13
failures are **identical on unmodified upstream 0.1.5** with the same data (checked in a separate worktree): golden /
real-data tests recorded against a different install than your US disc v3.2 (fonts, string tables, car catalogue,
golden car pages, driver camera, speech bank, and `nfs5.exe`, which this disc doesn't have). No regressions from the port.
With the 3.5 data: **1,388 passed, 19 failed**. The 6 new failures are layout counts that are exactly +2 (buttons,
gotos, stems, bevels, glinty boxes): the two "showcase…" buttons the 3.5 patch adds. A data difference, not a bug.

**Dial needles in the test screenshots (checked 2026-11-21):** the race stills sent overnight come from command-line
races (`open-unleashed alps 993 --screenshot …`). That path is the original's "no front end" launch, which leaves the dial
settings blank, so the faces and hub draw **without needles**. Upstream does this on purpose (`app/init.rs`, `hud.md`
§3.5). A race started from the menus, a career or a mission uses the car's own dials and draws both needles (checked:
mission 0M01, Boxster, mid-race). The tvOS app always starts races from the menus. No change made.

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
- **Patched to 3.5 (2026-11-21)** with the "Enhanced Patch v3.5" you linked (nfs-cars.com, `SETUP.EXE` = Wise installer,
  MD5 matched its `SIGFILE.MD5`). The installer was **never run**: its deflate streams were unpacked with a small script in
  the session scratch folder and matched to the install script's file list. Applied the data files only:
  `version.txt` (now `3.5`), `FEData/Layouts/{garage,loadreplay,usedcars}.lay` (adds the "showcase…" buttons),
  `FEData/Data/nfs5-{1,2}.hud`, `FEData/Locale/festrings.loc`, `properties.txt`. Also installed the patch's unprotected game
  executable as **`nfs5.exe`**: the engine only reads the movie decoder's tables from it (never runs it), so **the intro
  movies now play** (golden movie-frame test passes; seen in the tvOS Simulator). The 3.2 originals of the replaced
  files are in `game-data-v3.2-backup/` (git-ignored, like all `game-data-*`).
- **For Unraid:** copy the updated `game-data/` (incl. `nfs5.exe`) into `/mnt/user/appdata/nfs-haven-server/games/porsche/original` and restart the
  container. The Apple TV app then offers the update at its next launch (see TVOS.md, "Game data updates").

## Placeholders
Filled in: `<UNRAID_IP_OR_HOSTNAME>` = **192.168.50.37 / tower.local**, `<TAILSCALE_NAME>` = **tower-1.tawny-taipan.ts.net**
(used in `docs/nfs-haven-server.md`; the game's Server setting will default to `tower.local`).

Left for later (as asked): `<MY_APPLE_ID_OR_TEAM>`,
Apple TV paired `<yes/no>`, controller `<Xbox / PlayStation / other>`. Bundle ID: **`com.dannyelson.nfshaven`**
(given 2026-11-22; display name **NFS Haven**).

## Decisions to review
Choices made without asking (per your instruction); each is easy to change.
1. **winit**: patched 0.30 (`third_party/winit`) instead of upgrading to the 0.31 beta — smallest change, macOS untouched.
2. **SDL3 on tvOS**: video subsystem compiled in (never initialised) because SDL's UIKit code needs it to link.
3. **Engine split** into `lib.rs` + thin `main.rs` so tvOS links the same code as a static library.
4. **Disc**: `Porsche.exe` (SafeDisc) not copied as `nfs5.exe`. *Superseded by 34*: the 3.5 patch's unprotected executable is now `nfs5.exe`, so the intro movies play.
5. **Fork name** `dannyelson82/Open-Unleashed` (public, like every fork of a public repo); Unraid template URLs point at its `tvos-port` branch.
6. **Image tags**: `latest` + `edge` are published from pushes to `tvos-port` (and `main`), `X.Y.Z` from `nfs-haven-server-vX.Y.Z` tags, plus the short SHA — so the Unraid template's `:latest` works without a release.
7. **Save API paths** use `/files/`, `/versions/`, `/version/{id}/`, `/restore/{id}/` segments before the file path (the plan's shorter paths clash with file paths containing `/`).
8. **Lobby** accepts `?via=tailnet|lan` from the client, because behind Docker NAT the server may not see the device's real address.
9. **Bundle ID** `com.dannyelson.nfshaven` (yours, 2026-11-22; was a stand-in `org.openunleashed.tvos`); Simulator default "Apple TV 4K (4th generation)".
10. **Local Docker**: Colima (Homebrew, no admin rights) instead of Docker Desktop.
11. **Game Data screen in UIKit** (app shell, Swift) rather than drawn by the engine — the engine can't start without the data; focus engine gives Siri Remote + controller navigation for free.
12. **Launch check = sizes + a "verified" marker** of the last complete download (no hashing ~0.5 GB per launch); full SHA-256 only while downloading.
13. **ATS**: `NSAllowsLocalNetworking` + a narrow `ts.net` exception (plain HTTP inside WireGuard) so the tailnet name works even without `tailscale serve` HTTPS; HTTPS is still tried first for `*.ts.net`.
14. **Save mirror by polling** (every 15 s + on background, size/mtime stamp) instead of a hook in the engine's save code — no engine change; worst case a save made < 15 s before tvOS purges Caches *and* kills the app is lost locally (the server sync will cover it).
15. **Default server address** `tower.local`; token must be entered once.
16. **tvOS command line**: the engine ignores `-nfsh.*` UserDefaults launch arguments (they're the shell's).
17. **B / Menu = back on tvOS only**: the original's front end reads no pad; the Mac keeps that (authentic), tvOS needs a back button.
18. **Siri Remote Menu never leaves the app** (Apple's guideline wants Menu at the top level to go Home; for a sideloaded personal build the game's own "back"/exit question is more useful). Home (TV button) still leaves.
19. **On-screen keyboard opens by itself** when a text field becomes active (once per field); a chat box only on Y, since it is always focused.
20. **Pause on controller loss also on the Mac** (the shared `request_pause`; the keyboard could keep racing, but a pad vanishing mid-corner is worth a pause).
21. **Render resolution via the view's content scale** (system upscales) instead of an engine render target — default native until measured on the device.
22. **Siri Remote not counted as a racing pad** (matched by name "remote").
23. **Unsigned .ipa** for atvloadly (it signs with its own Apple ID) — nothing needs your Apple ID on the Mac.
24. **HUD size by a smaller "virtual screen"** for the HUD's layout: every element scales, the original's anchoring rules run unchanged; default Original on both platforms for now (the plan suggests Large on tvOS — decide on the TV).
25. **Extras opened from the pause menu first** (the race HUD's text drawing exists there); the main-menu entry follows.
26. **Head-look**: bumper/driver views only (the plan's "also in chase views" left out for now); recentre = race start, unpause, pad L3+R3 (no keyboard key bound); positive head yaw = view left — to confirm with AirPods.
27. **Traction control "On" left out**: it's a physics flag (`vehicle.tcs`) set at race start that replays and network peers don't know about, so it would make them diverge. Needs the flag recorded in the replay/launch data first (or an input-side throttle limiter once wheel-slip units are checked).
28. **Assists default off on both platforms** (the plan suggests a soft curve + medium speed steering on tvOS; decide with a pad on the TV).
29. **Save-sync conflict rules**: at start, both sides changed while apart → the server's copy is active, the local one is kept as a `.bak` file beside it (plan's rule). During play, another device saved since → **this device's copy wins** (it's the game being played; replacing the save under the running game could be overwritten again anyway) and the other device's copy stays on the server as a restorable version. One server namespace per game (`porsche`) for all of that game's profiles (was `household` before the NFS Haven rename).
30. **Sync is on by default on tvOS** once a server is set (it's the same server as the game data); **opt-in on the Mac** (Extras › Network › Sync saves).
31. **Lobby games join through the original's own list**: the server's games are probed unicast with the original's `'w'` probe instead of adding rows by hand — they appear with real name, version and ping, no "via server" marker, and a host found both ways shows once when its LAN address is the server's first (LAN requester); a tailnet requester may see a LAN host twice (100.x and LAN). Lobby is on whenever a server is set (independent of save sync).
32. **FXAA as the anti-aliasing option**, default **Off** on both platforms: a small pass of its own over the 3-D image (before the flare overlay and HUD, so text stays sharp). MSAA 4× not done yet; pick a default after seeing both on the TV.
33. **HD texture pack at 2×** as the target (≈ 378 MB as ASTC, vs ≈ 1.5 GB at 4×, most of it front-end art); world textures ASTC 6×6, the rest 4×4. Only the inventory exists so far.
34. **3.5 patch: data files only.** The patch's game executable became `nfs5.exe` (the engine only reads the movie tables from it) and the data files were applied; `gimme.dll`, the uninstaller and the SaveData HUD copies were not (Windows-only / per-profile). 3.2 originals kept in `game-data-v3.2-backup/`.
35. **tvOS launch-time data update check.** With a verified download and a server set, the app compares its manifest with the server's (gives up after ~4 s, so an offline Apple TV starts as before). If they differ, the Game Data screen offers **Update** (only changed files) or **Play** (keep the current files; asked again next launch).

36. **Server settings are per device**: Extras › Network (server, token, sync saves) lives in `network.json`, not in `extras.json`, because save sync shares `extras.json` between devices (the token shouldn't travel with the saves, and the Mac and the Apple TV may use different names). An older `extras.json` with a `network` section is read once. *Note:* the rest of `extras.json` (HUD size, graphics, assists…) **is** still shared between the Mac and the Apple TV through save sync — probably also better per device; not changed yet.
37. **Extras › Network in the game on both platforms**; on tvOS it writes through to the app shell's Server setting (one setting, two places to edit it). While the Extras menu is open it takes the whole keyboard (so typing "p" or "k" in a field doesn't pause or open photo mode); text rows start with the current text (Mac: Backspace to clear, Cmd+V pastes the clipboard's first line).

38. **"Token" is called "Password" everywhere you see it** (game, Apple TV screens, Unraid template, docs). Underneath it is still the same shared secret: `NFSH_TOKEN` in the container (was `OU_TOKEN`), `token` in `network.json`, a Bearer header, `?token=` on the status page — unchanged, so nothing set up already breaks.

39. **Clean rename, no aliases** (your instruction overrides `nfs-haven-plan.md`'s "keep the old names for one release"): no `OU_*` env, no `_ou-server._tcp`, no old set ids, no `ou-server` image tags.
40. **Bundle ID `com.dannyelson.nfshaven`** (yours; the plan said to keep the old one so an installed app updates in place — nothing is installed, so a fresh ID is fine). Internally the Xcode target, its folder and the `.app` inside the `.ipa` are still called `OpenUnleashed` (only you'd ever see that); the Home screen name is NFS Haven.
41. **The Mac app is "NFS Haven" too** (`packaging/brand.sh`, the window title in `brand.rs`, bundle ID the same as tvOS). Its game settings folder stays `~/Library/Application Support/PorscheUnleashed` (the engine's own, per game).
42. **Saves: one namespace per game**, `porsche` (was `household`); the save API itself is unchanged.
43. **Games without rules of their own are served whole** (every non-hidden file of `original/`); Porsche keeps its file rules. nfsse gets rules when ReNFS arrives.
44. **Server version 0.2.0** of its own (it had the engine's 0.1.5); first image tagged `nfs-haven-server-v0.2.0` as the plan says.
45. **Still in the Open-Unleashed fork** for now: the plan's separate `nfs-haven` repository (launcher, ReNFS) starts after the Apple TV test, as the plan's header says.

46. **Extras from the main menu: F12 (Mac keyboard), View/Back/Share/Select on a pad, and on tvOS also X / the Siri Remote's Play/Pause.** The plan names Play/Pause; SDL reports the remote's Play/Pause as its X button, so on tvOS a pad's X opens it in the menus too (X does nothing else there). The same keys close it. Not while a dialog, a transition, a text field or a binding row is active.

47. **HD packs: ASTC 4×4 only** (not the plan's 6×6 for world textures): the GPU takes block-compressed textures only at sizes that are a multiple of the block, and the game's textures are powers of two. The scan's projection now uses 4×4 throughout: **2× ≈ 464 MB** (was 378 MB with 6×6), 4× ≈ 1.86 GB — over the 1.5 GB budget, so 2× stays the target. Entries whose 2× size isn't a multiple of 4 stay RGBA8 in the pack.
48. **HD key = SHA-256 (first 16 bytes) of the entry's raw bytes**, the same hash the Stage 0 inventory already used (the plan suggested xxh3/blake3; SHA-256 is already a dependency and fast enough on Apple Silicon).
49. **Textures: Original / HD applies from the next race**, not live: switching mid-race would mean re-uploading every texture. A live A/B toggle can come with Stage 5.
50. **Cars stay original** in Stage 1 (car packs bypass the store): their entries are blitted onto pages at pixel positions, which needs pages built at scale — the plan's last step. Menu art too, for now.
51. **ARM's astcenc 5.7.1** (official macOS release, checksum verified) is downloaded into the git-ignored `target/tools/astcenc/` for the pack tool; nothing of it is committed.

52. **MSAA 4× without alpha-to-coverage**, and only for the 3-D pass: alpha-to-coverage would turn alpha into coverage on every blend-off material, and some keep other data in alpha (a car page's paint labels), which would punch holes. Cut-out edges (trees, fences) therefore stay as the original's alpha test draws them. FXAA and MSAA are one choice (Off / FXAA / MSAA 4×). A front-end frame whose sprites test the 3-D depth is drawn without MSAA (the depth they read is single-sampled).

53. **Haptics defaults**: tvOS Rumble Enhanced + Trigger effects Subtle (a pad is the only way to race there, as the plan suggests); the Mac Original + Off (authentic first). Both "apply now".
54. **Surface table is a guess** from how the original's own road rule treats each class (silent classes = smooth; 7/11 gravel, 5 kerb, 4 rough, 3/9 grass). The frame overlay now shows the class under the car, so it can be named per track on the TV and the table fixed.
55. **Wheelspin = the rear wheels' slip** (every car in the game is rear- or all-wheel drive); ABS felt only with the ABS assist on and a firm pedal; a wheel locked without it stiffens the brake trigger.
56. **DualSense adaptive triggers through Apple's GameController on the Mac too** (the plan's choice). On the Mac SDL may drive a DualSense through its own HIDAPI driver; if the triggers do nothing there, SDL's own `SDL_SendGamepadEffect` path is the fallback to try.

57. **5.1 scope**: Stereo (original) / "5.1 when available" (no separate 7.1 or Auto: the app can only use what the route gives; 7.1's back pair stays silent). Defaults: tvOS 5.1-when-available, Mac stereo. Sounds given no position (the player's own car, the front end) have angle 0 and play from the **centre**; music and stereo effects stay on L / R; no LFE feed, no "music in all speakers" yet. A route change after start (a receiver switched on later) is not re-read until the next start.

## Stage status
| Stage | macOS | tvOS Simulator | Apple TV |
|---|---|---|---|
| 1 Baseline build | ✅ release builds; runs; without data exits with the clear "game data not found" error | — | — |
| 1 Race starts | ✅ `NFS5_GAME_PATH=game-data open-unleashed alps 993`: 20 s headless race (car accelerates, shifts 1→4); windowed `--screenshot` race with 3 opponents renders correctly on Metal (Apple M2 Pro) | — | — |
| 1 Engine as a library | ✅ `crates/open-unleashed` is now `lib.rs` + a 3-line `main.rs`; `run()` split into `build_app()` + event loop (desktop order unchanged) | ✅ builds as a static lib | — |
| 2 tvOS cross-compile | ✅ unaffected | ✅ static lib builds (winit 0.30 patched, SDL3 video on for tvOS) | ✅ device static lib (`aarch64-apple-tvos`) builds, and the unsigned device-SDK Xcode build links; signing + install need the team ID and the Apple TV |
| 3 App shell (Xcode/XcodeGen, `scripts/build-tvos.sh`) | — | ✅ builds, installs, launches on "Apple TV 4K (4th generation)" Simulator, tvOS 27.2 | 📺 |
| 3 Engine runs | ✅ | ✅ **front end renders** (main menu + profile prompt) and **a race runs** (`alps 993 --opponents 3`: in-car view, HUD, mirror, race clock) from your disc's data copied into the Simulator app's Caches/game; audio output opens (CoreAudio 48 kHz stereo); SDL joystick layer starts | 📺 |
| 4 Game Data screen (Swift: `GameDataViewController`, `GameStore`, `ServerClient`, `Bonjour`) | — | ✅ fresh install → screen → downloads all 1,593 files (529 MB) from the server at ~79 MB/s with overall/per-file progress, SHA-256 per file, then the engine starts (main menu). Killed mid-download (306 MB + 76.5 MB partial) → relaunch completes and verifies. Server/token in UserDefaults, Find on network (Bonjour), Test connection, data-set choice (original/mod sets), clear errors (unreachable / token / disk full / hash mismatch) | 📺 LAN + Tailscale download from tower |
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
| 7.1 Frame-time overlay (`NFS_FPS_OVERLAY=1`, tvOS `-nfsh.fpsOverlay YES`) | ✅ | ✅ shows fps / avg / worst / budget (Simulator 60.0 fps — not representative) | 📺 real numbers |
| 7.2 60 Hz display criteria | — | 🟡 requested on scene activation (only acts with Match Frame Rate on) | 📺 |
| 7.3 Render resolution (`-nfsh.renderScale 1/1.5/2`) | — | ✅ surface 1760×960 / 2640×1440 / 3520×1920. Note: the window is 1760×960 points, not 1920×1080 — a TV safe-area inset in winit's window? To check (safe-area enhancement) | 📺 pick the default by measuring |
| 7.4–7.5 Shader stutter, Instruments profiling | — | — | 📺 |
| 8 `.ipa` for atvloadly (`scripts/export-ipa.sh`) | — | ✅ unsigned device build → `target/tvos/NFSHaven.ipa` (5.5 MB, checked: no game files) | 📺 install via atvloadly; check refresh keeps Caches + UserDefaults |
| 8 atvloadly docs (TVOS.md) | ✅ | — | — |

### Stage 9 — enhancements
| Item | macOS | tvOS Simulator | Apple TV |
|---|---|---|---|
| 0 Extras menu + `extras.json` (`nfs_game::extras`, `app/extras_ui.rs`) | ✅ pause menu's last item "extras"; pages/rows with value, explanation, "applies now/at next start", reset page; arrows/d-pad; saved + reloaded (tests); `extras.json` is in the tvOS save mirror | ✅ (same code; settings file read at start) | 📺 pad/remote navigation |
| 0 Extras › Network (server, password, sync saves, test connection) | ✅ scripted run: typed address + token, Sync on, Test → "Connected: Mac test, ou-server 0.1.5" (now "nfs-haven-server 0.2.0"); wrong password → "the server refused the password"; long Tailscale name fits; settings in `network.json`, none in `extras.json`. Also fixed: in the Extras menu the Mac keyboard's left/right had moved rows instead of changing values (the pause menu's mapping), and long help lines ran past the panel (now wrapped). 🙋 **check by hand**: real typing + Cmd+V (the scripted run feeds the same buffer, not real key events) | ✅ same page without Sync saves; Test → OK; the change reached the shell's setting (app container plist = `127.0.0.1:8642` / token). 🙋 the on-screen keyboard for these rows (same prompt as the other text fields) | 📺 |
| 0 Extras from the main menu (pad View / F12 / Siri Remote Play/Pause) | ✅ F12 opens/closes it over any front-end screen, a pad's View button too; it takes the keyboard, the pad and the mouse while up (the screen underneath gets nothing); drawn with the HUD's fonts loaded once for the front end. Screenshot over the garage. 🙋 F12 / View by hand | ✅ same over the garage (Network page shown); the Siri Remote's Play/Pause (SDL's X on the remote) also opens it — 📺 to confirm that button mapping on the device | 📺 |
| 2.1 Layered app icon + Top Shelf (+ Wide) from the project's own mark (`platform/tvos/make_brand_assets.py`, committed asset catalog) | — | ✅ icon on the Simulator's Home screen; 3 layers (opaque back, road, mark), App Store icon, Top Shelf images | 📺 parallax on focus, Top Shelf in the top row |
| 2.7 No screen saver during race / replay / intro (menus can still sleep) | — | 🟡 wired (shell polls `ou_wants_no_sleep()` every 3 s) | 📺 15-minute race without screen saver |
| 1 Controller glyphs (`nfs_input::glyphs`) | ✅ last device (key press / pad button) + pad family from SDL (Xbox / PlayStation / Nintendo / generic) + per-family button names (text in the game's font, no console artwork); photo-mode hints follow it (pad: "Menu: pause, then photo mode" + the pad's key line). ⬜ hint tokens for the career-market / race-result / replay hints, ⬜ SF Symbols atlas from the app shell, ⬜ glyphs on the controls-setup screen, ⬜ "Button icons" setting | same; tvOS starts on "pad" | 📺 check the hints with an Xbox pad and a DualSense |
| 2.2 HUD size Original / Large (1.5×) / Extra large (2×) | ✅ (pointer hover follows) | ✅ screenshots: every element scaled, original edge anchoring kept, nothing clipped | 📺 couch check on a 55″ TV |
| 7.1 Frame-time overlay switch in Extras | ✅ | ✅ | — |
| 6b-A/B/E Audio route handling + auto-pause | ✅ hot-swappable output: the mixer state is handed back when a stream is dropped and the new default device's stream continues every voice (unit test; all 176 audio tests pass, stereo path unchanged); route checked every 2 s (Mac: default device name) and on the stream's error; headphones → speakers mid-race pauses a single-player race (Extras › Audio, default on; network race: notice only). 🙋 **check**: switch the Mac's output / take an AirPod out mid-race — not done unattended (it changes a system setting and plays sound aloud) | ✅ builds: AVAudioSession route changes → `ou_audio_route_changed` (old device unavailable → pause) | 📺 AirPods on the Apple TV (probe P1/P2) |
| 6b-H Mac head-look (AirPods) | ✅ built in; filter unit-tested (5 tests: dead zone, 20°→75°, clamp, jump guard, look-key wins, drift); inside the app bundle CoreMotion starts ("motion updates started"); the bare binary refuses safely. 🙋 **needs you with AirPods**: direction, feel, 10-minute no-drift check | — (hidden: no API) | — |
| 4 Driving assists (`nfs_game::assists`, Extras › Driving assists) | ✅ steering response curve, speed-sensitive steering, countersteer assist (≤ 30 % lock, never fights you), ABS pulse; 5 unit tests; applied to the local live packet only. Replay/network exactness by construction (applied before the recorder/sender) — no separate A/B run yet. ⬜ braking assist (needs the AI's corner speed), ⬜ HUD "ABS/ASSIST" indicators, ⬜ presets row | same code | 📺 feel with a pad; 🙋 tuning |
| 3 Graphics: anisotropic 4/8/16× (scene trilinear samplers only), draw distance 125/150/200 % with fog pushed out, FOV −5…+10° (chase / in-car) | ✅ A/B screenshots: wider view and clearer far mountains as expected; anisotropy needs a look on the TV. ✅ **FXAA** (own small pass over the 3-D image, before the flare overlay and HUD; Extras › Anti-aliasing): A/B crops show softer stair-steps on fence rails, flat areas untouched — judge strength on the TV. ✅ **MSAA 4×** (Extras › Anti-aliasing, third choice; next start): the 3-D pass multisampled and resolved; Alps A/B: car outlines visibly smoother, 1.5 % of pixels changed, no GPU errors; front end unaffected. ⬜ alpha-to-coverage (decision 52), ⬜ per-track pop-in check at 150/200 %, ⬜ "Recommended for Apple TV" preset (after measuring) | same code | 📺 perf with the preset |
| 11 HD texture pack — Stage 0 inventory (`tools/texture-pack scan`) | ✅ 12,683 texture entries (9,356 unique by content hash) in 664 files, 2 s. Projected pack (ASTC 4×4, mips): **2× ≈ 464 MB**, 4× ≈ 1.86 GB (1 GB of it front-end art). Alpha: 5,708 opaque, 2,549 full, 505 binary, 100 colour-key, 494 paint-label. Output in git-ignored `target/texture-pack/`. ✅ **Stage 1 engine side** (`nfs_assets::hd`): pack index + KTX2 (RGBA8 / ASTC 4×4), entry key shared with the tool, upload paths for tracks + `.env`, sky, `common.fsh`, `particle.fsh`, room; ASTC upload in the renderer (feature requested where the GPU has it); Extras › Graphics › Textures; overlay counter; `NFS_HD_DUMP=1` → `hd-used.csv` (Alps race: 311 entries). ✅ **stand-in pack** `texture-pack fake` (Alps: 186 entries, 69 MB RGBA8 / **17.6 MB ASTC** via ARM astcenc 5.7.1): race shows **169 / 206** textures HD, layout, transparency and sky intact, nearest-neighbour blockiness up close as expected. ⬜ car pages at scale, ⬜ menu art, ⬜ live (in-race) toggle, ⬜ the Game Data screen fetching `porsche/hd`, ⬜ export / import, ⬜ upscaling (your Mac, your models) | ✅ same pack (ASTC) in the Simulator: 169 / 206, ASTC sampled | 📺 look + memory on the TV |
| 7.3 Render resolution in Extras (tvOS only) | — | ✅ row shown on tvOS only; read at window creation | 📺 |
| 5 Xbox/PlayStation pads via SDL's GameController driver, rumble | — | — | 📺 (no pads in the Simulator) |
| 6 5.1 surround (Extras › Audio › Speakers, Speaker test) | ✅ mixer: VBAP over L −30°, C 0°, R +30°, Ls −110°, Rs +110° from the angle the original folded away; stereo voices / music stay front, reverb wet also to the surrounds; runs only with ≥ 6 output channels; tests: a sound behind you comes only from Ls / Rs, one on the right from R + Rs, a stereo device gets the identical stereo mix; all 177 audio tests pass. Mac outputs stereo here (2 channels) | ✅ shell asks for the route's maximum channels and passes the order from the channel labels (`NFS_SPEAKER_ORDER`); the Simulator offers 2 → stereo | 📺 receiver: speaker test, AI behind you, a tunnel, route change |
| 5 Enhanced rumble + trigger effects (`nfs_game::haptics`, Extras › Controller) | ✅ built and unit-tested (6 tests): surface texture by class (guessed table), engine in the top 15 % of the revs, limiter buzz, gear-shift thump, original jolt kept; Xbox trigger rumble (`SDL_RumbleGamepadTriggers`, rate-limited); DualSense adaptive triggers through GameController (`nfs_input::dualsense_apple`: brake resistance → ABS vibration / lock stiffness, throttle wheelspin / limiter vibration), sent only on change, released on pause / race end / exit; "Test effects" row; the overlay shows the surface class. Race runs clean with it on. 🙋 **feel it with a pad** (none connected here) | ✅ builds; runs clean (defaults on: Enhanced + Subtle) | 📺 Xbox + DualSense feel, latency over Bluetooth, surface classes named per track |
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

## NFS Haven server (`tools/nfs-haven-server`, `docs/nfs-haven-server.md`; was ou-server)
| Part | State |
|---|---|
| `crates/nfs-haven-manifest` (shared) + `tools/make-manifest` | ✅ your data: 1,593 files, 528.8 MB, ~2 s; the server's manifest is byte-identical |
| Phase 1 — info + data sets (`original`, `mod-<name>` overlays, `hd`), manifests with hash cache, files with Range/If-Range/ETag | ✅ tested on the Mac: resumed download of the largest file (117.6 MB) verifies against its SHA-256; 416 past end; path escapes 400; files outside the manifest 404 |
| Phase 2 — saves (`If-Match`/412, versions: last N + one per day for 30 days, restore) | ✅ server side; tested (Mac/TV conflict keeps both, restore works) |
| Phase 2 client (`open-unleashed/src/sync.rs`, both platforms) | ✅ two fake "devices" on the Mac against the local server: A's save uploads; B (empty) pulls it at start; both changed while apart → server's copy active + local kept as `.local-<time>.bak` + server keeps the other as a version; another device saved during play → this device's copy current, the other kept as a server version. Offline: retried every 5 s. On tvOS on automatically with the Game Data server; on the Mac opt-in (`extras.json` › `network`: `server`, `token`, `sync_saves`) | 
| Phase 3 — lobby (register/heartbeat/expiry, address order for tailnet vs LAN, `?via=`) | ✅ server side; tested |
| Phase 3 client (`open-unleashed/src/server_lobby.rs`) | ✅ a hosted session (front end CREATE or `--net host`) registers with every local address, heartbeats every 5 s (players, racing), is removed when it ends; the games list also probes the server's games unicast, so they show up in the original's list with real name/ping and JOIN works unchanged. Test: host on port 9500 → server lists it; `--net-browse` finds it **only** with the server set ("HOSTMAC's race 10.0.0.186:9500 ping 29") | 
| Status page (`/?token=…`): sets, saves + versions with download/restore, games, devices | ✅ |
| Auth (Bearer / `?token=`), PUID/PGID drop, `--healthcheck` | ✅ |
| Tests | ✅ `cargo test -p nfs-haven-manifest -p nfs-haven-server`: 11 passed |
| Docker image (`tools/nfs-haven-server/Dockerfile`, context = repo root) | ✅ built and run locally (Colima, arm64): healthy, runs as UID 99, real game data mounted read-only, mod overlay + save verified |
| GitHub Actions → `ghcr.io/dannyelson82/nfs-haven-server` (`.github/workflows/nfs-haven-server-image.yml`, same pattern/pinned actions as ReelHaven) | ✅ **published**: fork `github.com/dannyelson82/Open-Unleashed` (branch `tvos-port`), run 44932561662 green (build, smoke test, Trivy, publish); tags `latest`, `edge`, `sha-5a34ce2`; package **public** (Unraid pulls without login). First run failed only on a test bug (process-name match) — fixed |
| Unraid template `packaging/unraid/nfs-haven-server.xml` (all under `/mnt/user/appdata/nfs-haven-server`) | ✅ written, XML-checked; you add the container yourself |
| Compose file, Bonjour (avahi service file), Tailscale (a/b, `tailscale serve` HTTPS) docs | ✅ |
| Phase 4 (dedicated host, EA lobby revival) | ⬜ research only, not started |

Local tools installed for this: Colima + Docker CLI + buildx (Homebrew, user-local; `colima start` / `colima stop`).

## Next (in order)
1. ✅ Extras from the main menu.
2. ✅ HD-pack Stage 1 engine side + stand-in pack (cars / menu art / Game Data download of `porsche/hd` still to do).
3. ✅ MSAA 4× next to FXAA (alpha-to-coverage left out, decision 52).
4. ✅ Enhanced rumble, Xbox trigger rumble, DualSense adaptive triggers (code; feel to check with a pad 🙋 / 📺).
5. ✅ 5.1 surround (VBAP) + speaker test (7.1, "music in all speakers" and an LFE feed not done; decision 57).
6. Split-screen (two local players).
7. HDR output.
8. Per-device display/graphics settings (decision 36), assists polish (braking assist, HUD indicators, presets), "Recommended for Apple TV" preset.
9. Apple TV (📺) items once the device is here.

## Needs Apple TV
- Stage 3: install + launch on the device; check winit/UIKit behaviour on real hardware.
- Signing (also needs `<MY_APPLE_ID_OR_TEAM>`): device builds, `scripts/build-tvos.sh device`.

## Blockers
- (resolved) GHCR publishing — fork created and image published after your OK.
