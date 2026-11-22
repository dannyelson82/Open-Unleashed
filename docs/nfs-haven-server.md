# nfs-haven-server — the NFS Haven server

One container on your home server that the **NFS Haven** app (Apple TV and Mac) points at, on the local network
or over Tailscale. It serves **your own copies** of the games' data, for any number of games (one folder per game,
plus optional mod sets and HD packs), keeps every game's **saves** with version history, and runs a **lobby** so
network games can be found and joined over Tailscale. Code: `tools/nfs-haven-server` (+ `crates/nfs-haven-manifest`,
shared with `tools/make-manifest`). Plan: `nfs-haven-plan.md` Phase 5 (it was `ou-server` before; renamed cleanly,
no compatibility names kept, since nothing was installed yet).

> **LAN / tailnet only — never port-forward it to the internet.** The password is a shared secret for home use.
> The image contains no game file; your game data never leaves your devices/tailnet.

## Your setup
| | |
|---|---|
| Unraid (LAN) | `192.168.1.10` / `tower.local` |
| Unraid (Tailscale) | `tower.<your-tailnet>.ts.net` |
| Image | `ghcr.io/dannyelson82/nfs-haven-server:latest` |
| Container | `nfs-haven-server` (template `packaging/unraid/nfs-haven-server.xml`) |
| Port | `8642` |
| Folders | share `nfs-haven`: `/mnt/user/nfs-haven/{games,inbox,sources,staging}`; saves `/mnt/user/appdata/nfs-haven-server/data` |

In the app's **Server** setting use `tower.local` (or `192.168.1.10`) at home and
`tower.<your-tailnet>.ts.net` away from home, plus the password. Where to enter it:

- **In the game (both platforms):** pause a race › **extras** › page **Network**: *Server*, *Password* (Enter / A to
  type: the keyboard on the Mac, where Cmd+V pastes; the on-screen keyboard on Apple TV), *Sync saves* (Mac only;
  Apple TV always syncs once a server is set) and *Test connection*. Takes effect at the next start.
- **Apple TV, before the game data is there:** the Game Data screen's *Server* button. Both places edit the same
  setting.

The address and password are kept per device (`network.json` beside `extras.json`, never synced), so the Mac and the
Apple TV can use different names for the same server.

## The games folder
```
/mnt/user/nfs-haven/   (and /mnt/user/appdata/nfs-haven-server/data for data/)
  games/                      mounted at /games (imports publish here)
    porsche/                  Need for Speed: Porsche Unleashed
      original/               GameData/, FEData/, version.txt, fe.txt, nfs5.exe   → set porsche/original
      mods/<name>/            GameData/, FEData/, SaveData/ as the mod ships them   → set porsche/mod-<name>
      hd/                     the HD texture pack                                  → set porsche/hd
    nfsse/                    (later) The Need for Speed SE
      original/               the CD's files                                       → set nfsse/original
  data/                       saves (one namespace per game: saves/porsche/…), versions, caches
```
A **new game is a new folder** (lower-case letters, digits, `-`): no code change. Porsche's `original/` and mods are
read with its rules (only the folders and top-level files the engine uses; `docs/en/game-data.md`); any other game's
folder is served whole until it gets rules of its own (`nfs_haven_manifest::kinds_for`). A mod set is the game's
original with the mod's files on top; the original is never changed.

## Imports: drop a disc image, get a game

Drop your own **disc image** or an **official update** into `inbox/` (over SMB, Finder or the Unraid share). The
server looks every few seconds; a file that hasn't changed between two looks is taken:

1. **recognised by its contents**, not its name: a disc image (`.img` / `.bin` raw 2352-byte sectors, `.bin` + `.cue`,
   `.iso`, read in place — no conversion on disk) is opened and its file system read (ISO 9660 with Joliet): a CD with
   `NFS_DOS.EXE` is **The Need for Speed SE**, one with `GameData/` and `FEData/` is **Porsche Unleashed**; an update
   is known by its SHA-256 (also inside the `.zip` it was downloaded in). Anything else stays in the inbox, listed as
   "not recognised".
2. **kept**: moved to `sources/<game>/`, made read-only, its SHA-256 recorded (`sources/<game>/sources.json`). The
   same file again is noticed by its checksum and simply removed from the inbox.
3. **built**: the game's recipe runs in `staging/` from all its kept sources —
   - `nfsse`: the whole CD; `NFS_DOS.EXE` must be one of the two builds ReNFS runs (its own check, ported: the loaded
     program's FNV-1a is `0x76078ed2` or `0x21ef7c33`);
   - `porsche`: the files the engine reads from the disc (`GameData/`, `FEData/`, `version.txt`, `fe.txt`, …), then each
     known update applied — the **3.5 update** ("Enhanced Version 3.5.20040310", `SETUP.EXE` or its download `.zip`) is
     unpacked **without running it**: its files are found among the installer's compressed streams by their checksums
     (`version.txt` 3.5, three layouts, two HUD files, `festrings.loc`, `properties.txt`) and its game executable is
     installed as `nfs5.exe` (the engine only reads the movie tables from it).
4. **verified** (every file's size against the disc, the update's files against their checksums, the program check)
   with a free-space check first, then **swapped into `games/<game>/original` in one step** (a directory exchange; the
   previous set stays until the new one is complete, then goes). The manifests rebuild at once, so the apps see an
   update. A failed import changes nothing and leaves nothing in staging.

The **status page** has an Imports section: the inbox, each game's sources (with **Re-import**, which rebuilds from the
kept sources), and the recent jobs with their logs (`GET /v1/imports`, `POST /v1/imports/{game}/run`,
`GET /v1/imports/logs/{id}`). Each published set carries a hidden `.nfsh-import.json` saying which sources made it.

## Install on Unraid
1. Either drop your disc images and the 3.5 update into `/mnt/user/nfs-haven/inbox/` (the `nfs-haven` share's `inbox`) once the container
   runs (Imports, above), or copy a game's files to `games/<game>/original/` yourself, e.g. from the Mac:
   `rsync -a ~/Documents/OpenUnleashed/game-data/ root@tower.local:/mnt/user/nfs-haven/games/porsche/original/`
2. Make the share `nfs-haven` (Shares › Add Share, SMB export Yes, Private, your user Read/Write). Install the template
   once (Unraid terminal: `wget -O /boot/config/plugins/dockerMan/templates-user/my-nfs-haven-server.xml
   https://raw.githubusercontent.com/dannyelson82/Open-Unleashed/tvos-port/packaging/unraid/nfs-haven-server.xml`),
   then Docker › Add Container › Template › **nfs-haven-server**.
3. Set **Password** to a long random string (e.g. `openssl rand -hex 24`). Leave the paths as they are.
4. Apply, then open `http://tower.local:8642/?token=<password>`: the status page ("NFS Haven server") lists
   `porsche/original` (1,594 files, 531 MB for the US disc patched to 3.5) once the manifest is built (a few seconds;
   hashes are cached in `data/cache`, so restarts are instant).

Backups: `data/` holds the saves and their versions — the normal Unraid appdata backup covers it.

## Settings (environment)
| Variable | Default | |
|---|---|---|
| `NFSH_TOKEN` | — | the **Password** (template field): shared secret; the app sends `Authorization: Bearer <password>` (status page: `?token=<password>`). Empty = open (warned) |
| `NFSH_PORT` | `8642` | |
| `NFSH_NAME` | `NFS Haven server` | shown in the app |
| `NFSH_GAMES_DIR` | `/games` | one folder per game (above) |
| `NFSH_DATA_DIR` | `/data` | saves, versions, caches, import logs |
| `NFSH_INBOX_DIR` | `/inbox` | drop folder for disc images and updates (imports are off without it) |
| `NFSH_SOURCES_DIR` | `/sources` | the imported files, kept read-only, one folder per game |
| `NFSH_STAGING_DIR` | `/staging` | where an import builds a set (same file system as `/games` saves a copy) |
| `NFSH_INBOX_POLL_SECS` | `5` | how often the inbox is looked at |
| `NFSH_SAVE_VERSIONS` | `20` | earlier copies per save file (+ the newest of each of the last 30 days) |
| `NFSH_LOBBY_TTL` | `15` | seconds without a heartbeat before a game leaves the lobby |
| `NFSH_RESCAN_SECS` | `60` | how often the folders are checked for changes (manifests rebuilt on change) |
| `NFSH_SAVE_MAX_BYTES` | `8388608` | size limit per save file |
| `PUID` / `PGID` / `UMASK` | `99` / `100` / `022` | the server drops root at start |

## API (`/v1`, JSON, `Authorization: Bearer <password>`)
| | |
|---|---|
| `GET /healthz` | `ok` (no password; container health check: `nfs-haven-server --healthcheck`) |
| `GET /v1/info` | name, `server: "nfs-haven-server"`, version, features, `games`, data sets, lobby |
| `GET /v1/data/sets` | every set (`porsche/original`, `porsche/mod-<name>`, `porsche/hd`, …) with its game, kind, file count and size |
| `GET /v1/data/{game}/{set}/manifest` | `{format, set, total_size, files:[{path,size,sha256}]}` — identical to `make-manifest --game {game}`'s output |
| `GET /v1/data/{game}/{set}.tar` | **fast install**: the whole set as one uncompressed tar, streamed on the fly in manifest order (nothing extra on disk); fixed ustar headers, so every file's offset follows from the manifest; `Range` + `If-Range` on the `ETag` `"tar-<16 bytes of the manifest's SHA-256>"` — the app resumes from its first missing file and checks every file's SHA-256 as it extracts |
| `GET /v1/data/{game}/{set}/file/{path}` | the file; `Range` (resume), `If-Range`, `ETag` = `"<sha256>"`, `X-Content-SHA256`. Only manifest files; no path escapes |
| `GET /v1/saves` | namespaces (one per game: `porsche`, …) |
| `GET /v1/saves/{ns}` | files with size, SHA-256, ETag, last writer (`X-NFSH-Device`), time |
| `GET /v1/saves/{ns}/files/{file}` | the current copy |
| `PUT /v1/saves/{ns}/files/{file}` | store; **`If-Match: "<sha256>"`** of the copy the device last had (none for a new file). Mismatch → **412** with the server's ETag — nothing is overwritten; the app keeps both. Same bytes → 200 "unchanged" |
| `GET /v1/saves/{ns}/versions/{file}` | earlier copies |
| `GET /v1/saves/{ns}/version/{id}/{file}` | one earlier copy |
| `POST /v1/saves/{ns}/restore/{id}/{file}` | make it current again (the current copy becomes a version) |
| `POST /v1/lobby/games` | a host registers `{name, host_player, track, mode, laps, players, max_players, version, addresses[], port}` → `{id}` (Porsche only for now) |
| `PUT /v1/lobby/games/{id}` | heartbeat every 5 s (`{players, started, track}` optional) |
| `DELETE /v1/lobby/games/{id}` | the session closed |
| `GET /v1/lobby/games[?via=tailnet\|lan]` | open games; each game's addresses ordered for the asker (tailnet 100.x first for a tailnet requester) |
| `GET /v1/imports` | inbox, kept sources per game, recent import jobs |
| `POST /v1/imports/{game}/run` | re-import a game from its kept sources |
| `GET /v1/imports/logs/{id}` | one job's log |
| `GET /` | status page |

Porsche's save files kept: `SaveData/*.sav`, `SaveData/nfs5.trk`, `SaveData/*.json`, `career.json`, `settings.json`
(the server also accepts `extras.json`, but the game keeps its Extras settings per device and no longer sends it). Replays and photos stay on the device.

## Tailscale
- **(a) recommended — Unraid itself on the tailnet** (it already is: `tower.<your-tailnet>.ts.net`): the container's
  port is reachable at `http://tower.<your-tailnet>.ts.net:8642`.
- (b) a `tailscale/tailscale` sidecar sharing the container's network, giving it its own name `nfs-haven-server`.

**HTTPS for the tailnet name (Apple TV):** App Transport Security allows plain HTTP to local-network addresses
(`NSAllowsLocalNetworking`), but a `*.ts.net` name may not count as local. On Unraid: `tailscale serve --bg --https=443
http://127.0.0.1:8642` gives `https://tower.<your-tailnet>.ts.net/` a valid certificate (enable HTTPS certificates in
the Tailscale admin console first). The app tries HTTPS first for `*.ts.net` names; a narrow ATS exception for
`ts.net` in the app's Info.plist is the fallback.

## Finding the server on the LAN (Bonjour)
The app can find the server by itself with Bonjour `_nfs-haven._tcp`. A container on Unraid's bridge network can't
advertise mDNS itself; Unraid's own avahi can. Save this as `/etc/avahi/services/nfs-haven-server.service` (and
re-create it at boot from `/boot/config/go`, since `/etc` is in RAM):
```xml
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name>NFS Haven server</name>
  <service><type>_nfs-haven._tcp</type><port>8642</port><txt-record>api=1</txt-record></service>
</service-group>
```

## Building and publishing
- Locally: `docker build -f tools/nfs-haven-server/Dockerfile -t nfs-haven-server .` (context = repository root), or
  `cargo run --release -p nfs-haven-server` with `NFSH_GAMES_DIR=…` etc.
- CI: `.github/workflows/nfs-haven-server-image.yml` builds, smoke-tests (health, auth, manifests for two games,
  Range, mod overlay, save conflict, PUID 99), scans (Trivy) and publishes `ghcr.io/dannyelson82/nfs-haven-server` —
  `latest`/`edge` from the working branch, `X.Y.Z` from `nfs-haven-server-vX.Y.Z` tags, plus the short SHA.
- Tests: `cargo test -p nfs-haven-manifest -p nfs-haven-server`.
- Compose: `tools/nfs-haven-server/docker-compose.yml`.
