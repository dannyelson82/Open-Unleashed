# ou-server — the OpenUnleashed server

One container on your home server that the OpenUnleashed game (Apple TV and Mac) points at, on the local network
or over Tailscale. It serves **your own copy** of the game data (plus optional mod sets and an HD texture pack),
keeps every profile's **saves** with version history, and runs a **lobby** so network games can be found and
joined over Tailscale. Design: `server-plan.md`. Code: `tools/ou-server` (+ `crates/ou-manifest`, shared with
`tools/make-manifest`).

> **LAN / tailnet only — never port-forward it to the internet.** The token is a shared secret for home use.
> The image contains no game file; the game data is mounted read-only and never leaves your devices/tailnet.

## Your setup
| | |
|---|---|
| Unraid (LAN) | `192.168.50.37` / `tower.local` |
| Unraid (Tailscale) | `tower-1.tawny-taipan.ts.net` |
| Image | `ghcr.io/dannyelson82/ou-server:latest` |
| Port | `8642` |
| Data | `/mnt/user/appdata/ou-server/` (`game/`, `mods/`, `hd/`, `data/`) |

In the game's **Server** setting use `tower.local` (or `192.168.50.37`) at home and
`tower-1.tawny-taipan.ts.net` away from home, plus the token. Where to enter it:

- **In the game (both platforms):** pause a race › **extras** › page **Network**: *Server*, *Token* (Enter / A to
  type: the keyboard on the Mac, where Cmd+V pastes; the on-screen keyboard on Apple TV), *Sync saves* (Mac only;
  Apple TV always syncs once a server is set) and *Test connection*. Takes effect at the next start.
- **Apple TV, before the game data is there:** the Game Data screen's *Server* button. Both places edit the same
  setting.

The address and token are kept per device (`network.json` beside `extras.json`, never synced), so the Mac and the
Apple TV can use different names for the same server.

## Install on Unraid
1. Copy your game folder's `GameData/`, `FEData/`, `version.txt` and `fe.txt` to `/mnt/user/appdata/ou-server/game/`
   (e.g. from the Mac: `rsync -a ~/Documents/OpenUnleashed/game-data/ root@tower.local:/mnt/user/appdata/ou-server/game/`).
2. Docker › Add Container › Template: paste the template URL, or copy `packaging/unraid/ou-server.xml` to
   `/boot/config/plugins/dockerMan/templates-user/my-ou-server.xml` on the flash drive and pick it.
3. Set **Token** to a long random string (e.g. `openssl rand -hex 24`). Leave the paths as they are.
4. Apply, then open `http://tower.local:8642/?token=<token>` — the status page lists the data set
   (`original`, 1,593 files, 528.8 MB for the US disc) once the manifest is built (a few seconds; hashes are cached
   in `data/cache`, so restarts are instant).

Mod sets: one folder per set in `/mnt/user/appdata/ou-server/mods/<name>/` containing `GameData/`, `FEData/` and/or
`SaveData/` as the mod ships them; it appears as the data set `mod-<name>` (the original's files with the mod's on
top; the original set is never changed). The HD pack goes into `hd/`.

Backups: `data/` holds the saves and their versions — the normal Unraid appdata backup covers it.

## Settings (environment)
| Variable | Default | |
|---|---|---|
| `OU_TOKEN` | — | shared secret; the game sends `Authorization: Bearer <token>` (status page: `?token=`). Empty = open (warned) |
| `OU_PORT` | `8642` | |
| `OU_NAME` | `OpenUnleashed server` | shown in the game |
| `OU_SAVE_VERSIONS` | `20` | earlier copies per save file (+ the newest of each of the last 30 days) |
| `OU_LOBBY_TTL` | `15` | seconds without a heartbeat before a game leaves the lobby |
| `OU_GAME_DIR` / `OU_MODS_DIR` / `OU_HD_DIR` / `OU_DATA_DIR` | `/game` `/mods` `/hd` `/data` | |
| `OU_RESCAN_SECS` | `60` | how often the folders are checked for changes (manifests rebuilt on change) |
| `OU_SAVE_MAX_BYTES` | `8388608` | size limit per save file |
| `PUID` / `PGID` / `UMASK` | `99` / `100` / `022` | the server drops root at start |

## API (`/v1`, JSON, `Authorization: Bearer <token>`)
| | |
|---|---|
| `GET /healthz` | `ok` (no token; container health check: `ou-server --healthcheck`) |
| `GET /v1/info` | name, version, features, data sets, lobby |
| `GET /v1/data/sets` | `original`, `mod-<name>`…, `hd` with file counts and sizes |
| `GET /v1/data/{set}/manifest` | `{format, set, total_size, files:[{path,size,sha256}]}` — identical to `make-manifest`'s output |
| `GET /v1/data/{set}/file/{path}` | the file; `Range` (resume), `If-Range`, `ETag` = `"<sha256>"`, `X-Content-SHA256`. Only manifest files; no path escapes |
| `GET /v1/saves` | profiles |
| `GET /v1/saves/{profile}` | files with size, SHA-256, ETag, last writer (`X-OU-Device`), time |
| `GET /v1/saves/{profile}/files/{file}` | the current copy |
| `PUT /v1/saves/{profile}/files/{file}` | store; **`If-Match: "<sha256>"`** of the copy the device last had (none for a new file). Mismatch → **412** with the server's ETag — nothing is overwritten; the game keeps both. Same bytes → 200 "unchanged" |
| `GET /v1/saves/{profile}/versions/{file}` | earlier copies |
| `GET /v1/saves/{profile}/version/{id}/{file}` | one earlier copy |
| `POST /v1/saves/{profile}/restore/{id}/{file}` | make it current again (the current copy becomes a version) |
| `POST /v1/lobby/games` | a host registers `{name, host_player, track, mode, laps, players, max_players, version, addresses[], port}` → `{id}` |
| `PUT /v1/lobby/games/{id}` | heartbeat every 5 s (`{players, started, track}` optional) |
| `DELETE /v1/lobby/games/{id}` | the session closed |
| `GET /v1/lobby/games[?via=tailnet\|lan]` | open games; each game's addresses ordered for the asker (tailnet 100.x first for a tailnet requester) |
| `GET /` | status page |

Save files kept: `SaveData/*.sav`, `SaveData/nfs5.trk`, `SaveData/*.json`, `career.json`, `extras.json`,
`settings.json`. Replays and photos stay on the device.

## Tailscale
- **(a) recommended — Unraid itself on the tailnet** (it already is: `tower-1.tawny-taipan.ts.net`): the container's
  port is reachable at `http://tower-1.tawny-taipan.ts.net:8642`.
- (b) a `tailscale/tailscale` sidecar sharing the container's network, giving it its own name `ou-server`.

**HTTPS for the tailnet name (Apple TV):** App Transport Security allows plain HTTP to local-network addresses
(`NSAllowsLocalNetworking`), but a `*.ts.net` name may not count as local. On Unraid: `tailscale serve --bg --https=443
http://127.0.0.1:8642` gives `https://tower-1.tawny-taipan.ts.net/` a valid certificate (enable HTTPS certificates in
the Tailscale admin console first). The game tries HTTPS first for `*.ts.net` names; a narrow ATS exception for
`ts.net` in the app's Info.plist is the fallback.

## Finding the server on the LAN (Bonjour)
The game can find the server by itself with Bonjour `_ou-server._tcp`. A container on Unraid's bridge network can't
advertise mDNS itself; Unraid's own avahi can. Save this as `/etc/avahi/services/ou-server.service` (and re-create
it at boot from `/boot/config/go`, since `/etc` is in RAM):
```xml
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name>OpenUnleashed server</name>
  <service><type>_ou-server._tcp</type><port>8642</port><txt-record>api=1</txt-record></service>
</service-group>
```

## Building and publishing
- Locally: `docker build -f tools/ou-server/Dockerfile -t ou-server .` (context = repository root), or
  `cargo run --release -p ou-server` with `OU_GAME_DIR=…` etc.
- CI: `.github/workflows/ou-server-image.yml` builds, smoke-tests (health, auth, manifest, Range, mod overlay, save
  conflict, PUID 99), scans (Trivy) and publishes `ghcr.io/dannyelson82/ou-server` — `latest`/`edge` from the working
  branch, `X.Y.Z` from `ou-server-vX.Y.Z` tags, plus the short SHA.
- Tests: `cargo test -p ou-manifest -p ou-server`.
- Compose: `tools/ou-server/docker-compose.yml`.
