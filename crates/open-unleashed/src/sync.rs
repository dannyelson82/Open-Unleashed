//! **PLATFORM GLUE (save sync, `server-plan.md` phase 2)** — the saves shared between the Apple TV and the Mac
//! through the OpenUnleashed server (`tools/ou-server`). Off until a server is set (Extras `network`, or on tvOS the
//! app shell's Server setting as `OU_SERVER` / `OU_TOKEN`); with no server everything is as before.
//!
//! * **Pull at start**, before the engine reads any save (`run`/`build_app`), a few seconds at most: files the
//!   other device changed come down.
//! * **Push after saving**: a worker thread notices changed save files (sizes + modification times, every 5 s)
//!   and uploads them with `If-Match` on the copy this device last saw.
//! * **Conflicts never lose a save.** At start (both sides changed while apart): the server's copy becomes the
//!   active one and the local one is kept beside it as `<file>.local-<time>.bak`. During play (another device
//!   saved since): this device's copy — the game being played — becomes current; the server keeps the other copy
//!   as a version (restorable from its status page).
//! * **Offline**: changes wait and are retried every 5 s; the NSUserDefaults mirror (tvOS) stays the last resort.
//!
//! The files are those the server keeps: `SaveData/*.sav`, `SaveData/nfs5.trk`, `SaveData/*.json`, `career.json`,
//! `extras.json`. The sync state (what was last agreed with the server) is `.ou-sync.json` in the data folder.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct SyncConfig {
    bases: Vec<String>,
    token: String,
    device: String,
    profile: String,
}

/// Base URLs to try for an address (as the tvOS shell does): `*.ts.net` → HTTPS first, else plain HTTP on 8642.
fn candidates(addr: &str) -> Vec<String> {
    let a = addr.trim().trim_end_matches('/');
    if a.is_empty() {
        return Vec::new();
    }
    if a.starts_with("http://") || a.starts_with("https://") {
        return vec![a.to_string()];
    }
    let (host, port) = match a.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && p.chars().all(|c| c.is_ascii_digit()) => (h, Some(p)),
        _ => (a, None),
    };
    let mut v = Vec::new();
    if host.to_ascii_lowercase().ends_with(".ts.net") {
        v.push(match port {
            Some(p) => format!("https://{host}:{p}"),
            None => format!("https://{host}"),
        });
    }
    v.push(format!("http://{host}:{}", port.unwrap_or("8642")));
    v
}

/// The save sync's settings: a server set **and** sync on (tvOS: on whenever the shell passes a server).
fn config() -> Option<SyncConfig> {
    let n = nfs_game::extras::get().network;
    let enabled = std::env::var("OU_SERVER").is_ok_and(|v| !v.trim().is_empty()) || n.sync_saves;
    if !enabled || std::env::var_os("NFS_NO_SYNC").is_some() {
        return None;
    }
    server_config()
}

/// The OpenUnleashed server, if one is set (the lobby needs only this).
pub(crate) fn server_config() -> Option<SyncConfig> {
    let n = nfs_game::extras::get().network;
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let server = env("OU_SERVER").unwrap_or(n.server.clone());
    if server.trim().is_empty() {
        return None;
    }
    let device = env("OU_DEVICE").unwrap_or_else(|| {
        if cfg!(target_os = "tvos") { "Apple TV".into() } else { std::env::var("HOST").or_else(|_| std::env::var("USER")).map(|u| format!("Mac ({u})")).unwrap_or_else(|_| "Mac".into()) }
    });
    let profile = if n.save_profile.trim().is_empty() { "household".to_string() } else { n.save_profile.trim().to_string() };
    Some(SyncConfig { bases: candidates(&server), token: env("OU_TOKEN").unwrap_or(n.token), device, profile })
}

/// One file's agreed state: the SHA-256 both sides had at the last sync, and the local stamp it had then.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct Agreed {
    sha256: String,
    stamp: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    files: BTreeMap<String, Agreed>,
}

fn state_path() -> PathBuf {
    nfs_assets::platform::data_dir().join(".ou-sync.json")
}

fn load_state() -> State {
    std::fs::read(state_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_state(s: &State) {
    let p = state_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(s).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(tmp, p);
    }
}

/// Server file name → local path.
fn local_path(file: &str) -> PathBuf {
    match file.split_once('/') {
        Some((_, name)) => nfs_game::save::save_dir().join(name),
        None => nfs_assets::platform::data_dir().join(file),
    }
}

/// The local save files the server keeps (server name → path).
fn local_files() -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    for top in ["career.json", "extras.json"] {
        let p = nfs_assets::platform::data_dir().join(top);
        if p.is_file() {
            out.insert(top.to_string(), p);
        }
    }
    if let Ok(rd) = std::fs::read_dir(nfs_game::save::save_dir()) {
        for e in rd.flatten() {
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            let l = name.to_ascii_lowercase();
            if e.path().is_file() && !name.starts_with('.') && (l.ends_with(".sav") || l == "nfs5.trk" || l.ends_with(".json")) {
                out.insert(format!("SaveData/{name}"), e.path());
            }
        }
    }
    out
}

fn stamp(p: &Path) -> String {
    std::fs::metadata(p).map(|m| format!("{}:{:?}", m.len(), m.modified().ok())).unwrap_or_default()
}

fn sha(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A connection: the first base URL that answers.
pub(crate) struct ServerConn {
    agent: ureq::Agent,
    base: String,
    cfg: SyncConfig,
}

#[derive(Debug, Deserialize)]
struct RemoteFile {
    file: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct RemoteList {
    files: Vec<RemoteFile>,
}

/// A path segment, percent-encoded.
fn seg(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

impl ServerConn {
    pub(crate) fn connect(cfg: &SyncConfig, timeout: Duration) -> Option<Self> {
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into();
        for b in &cfg.bases {
            let ok = agent
                .get(format!("{b}/v1/info"))
                .header("Authorization", format!("Bearer {}", cfg.token))
                .call()
                .is_ok_and(|r| r.status().is_success());
            if ok {
                return Some(Self { agent, base: b.clone(), cfg: cfg.clone() });
            }
        }
        None
    }

    pub(crate) fn base(&self) -> &str {
        &self.base
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.cfg.token)
    }

    /// The lobby's open games: each one's best address for this device (`?via=` says how it reached the server).
    pub(crate) fn lobby_games(&self) -> Option<Vec<std::net::SocketAddr>> {
        let host = self.base.split("://").nth(1).unwrap_or("").split([':', '/']).next().unwrap_or("");
        let via = if host.ends_with(".ts.net") || host.starts_with("100.") { "tailnet" } else { "lan" };
        let mut r = self.agent.get(format!("{}/v1/lobby/games?via={via}", self.base)).header("Authorization", self.auth()).call().ok()?;
        if !r.status().is_success() {
            return None;
        }
        let v: serde_json::Value = r.body_mut().read_json().ok()?;
        let games = v.get("games")?.as_array()?;
        Some(
            games
                .iter()
                .filter_map(|g| {
                    let port = g.get("port").and_then(|p| p.as_u64()).unwrap_or(9442) as u16;
                    let ip: std::net::IpAddr = g.get("addresses")?.as_array()?.first()?.as_str()?.parse().ok()?;
                    Some(std::net::SocketAddr::new(ip, port))
                })
                .collect(),
        )
    }

    /// Register a hosted game; its id.
    pub(crate) fn lobby_register(&self, body: &serde_json::Value) -> Option<String> {
        let mut r = self.agent.post(format!("{}/v1/lobby/games", self.base)).header("Authorization", self.auth()).send_json(body).ok()?;
        if !r.status().is_success() {
            return None;
        }
        let v: serde_json::Value = r.body_mut().read_json().ok()?;
        v.get("id")?.as_str().map(str::to_string)
    }

    /// Heartbeat: `Some(true)` kept, `Some(false)` unknown on the server (expired), `None` unreachable.
    pub(crate) fn lobby_heartbeat(&self, id: &str, players: u32, started: bool) -> Option<bool> {
        let r = self
            .agent
            .put(format!("{}/v1/lobby/games/{}", self.base, seg(id)))
            .header("Authorization", self.auth())
            .send_json(serde_json::json!({ "players": players, "started": started }))
            .ok()?;
        match r.status().as_u16() {
            200..=299 => Some(true),
            404 => Some(false),
            _ => None,
        }
    }

    pub(crate) fn lobby_delete(&self, id: &str) {
        let _ = self.agent.delete(format!("{}/v1/lobby/games/{}", self.base, seg(id))).header("Authorization", self.auth()).call();
    }

    fn url(&self, kind: &str, file: &str) -> String {
        let f = file.split('/').map(seg).collect::<Vec<_>>().join("/");
        format!("{}/v1/saves/{}/{kind}/{f}", self.base, seg(&self.cfg.profile))
    }

    fn list(&self) -> Option<Vec<RemoteFile>> {
        let mut r = self.agent.get(format!("{}/v1/saves/{}", self.base, seg(&self.cfg.profile))).header("Authorization", format!("Bearer {}", self.cfg.token)).call().ok()?;
        if !r.status().is_success() {
            return None;
        }
        r.body_mut().read_json::<RemoteList>().ok().map(|l| l.files)
    }

    fn download(&self, file: &str) -> Option<Vec<u8>> {
        let mut r = self.agent.get(self.url("files", file)).header("Authorization", format!("Bearer {}", self.cfg.token)).call().ok()?;
        if !r.status().is_success() {
            return None;
        }
        r.body_mut().read_to_vec().ok()
    }

    /// PUT with `If-Match` (or none for a new file). `Ok(sha)` stored, `Err(Some(server_sha))` conflict,
    /// `Err(None)` unreachable / refused.
    fn upload(&self, file: &str, bytes: &[u8], if_match: Option<&str>) -> Result<String, Option<String>> {
        let mut req = self.agent.put(self.url("files", file)).header("Authorization", format!("Bearer {}", self.cfg.token)).header("X-OU-Device", &self.cfg.device);
        if let Some(m) = if_match {
            req = req.header("If-Match", format!("\"{m}\""));
        }
        let r = req.send(bytes).map_err(|_| None)?;
        let etag = r.headers().get("etag").and_then(|v| v.to_str().ok()).map(|e| e.trim_matches('"').to_string());
        match r.status().as_u16() {
            200 | 201 => Ok(etag.unwrap_or_else(|| sha(bytes))),
            412 => Err(etag),
            s => {
                tracing::warn!(file, status = s, "save sync: upload refused");
                Err(None)
            }
        }
    }
}

fn write_local(p: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("ou-sync-tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, p)
}

/// At start, before any save is read: bring down what changed elsewhere; keep both on a conflict.
pub fn pull_at_start() {
    let Some(cfg) = config() else { return };
    // TEST GLUE: `NFS_NO_SYNC_PULL=1` skips the start pull (to exercise the uploader's conflict path)
    if std::env::var_os("NFS_NO_SYNC_PULL").is_some() {
        return;
    }
    let t0 = std::time::Instant::now();
    let Some(c) = ServerConn::connect(&cfg, Duration::from_secs(3)) else {
        tracing::warn!(server = ?cfg.bases, "save sync: server unreachable at start — playing with the local saves");
        return;
    };
    let Some(remote) = c.list() else { return };
    let mut st = load_state();
    let (mut down, mut kept) = (0, 0);
    for rf in remote {
        let path = local_path(&rf.file);
        let local = std::fs::read(&path).ok();
        let agreed = st.files.get(&rf.file).cloned();
        let local_sha = local.as_deref().map(sha);
        if local_sha.as_deref() == Some(rf.sha256.as_str()) {
            st.files.insert(rf.file.clone(), Agreed { sha256: rf.sha256.clone(), stamp: stamp(&path) });
            continue;
        }
        let local_changed = match (&agreed, &local_sha) {
            (Some(a), Some(l)) => &a.sha256 != l,
            (None, Some(_)) => true,
            (_, None) => false,
        };
        let server_changed = agreed.as_ref().is_none_or(|a| a.sha256 != rf.sha256);
        if local_changed && !server_changed {
            continue; // ours is newer: the watcher pushes it
        }
        let Some(bytes) = c.download(&rf.file) else { continue };
        if local_changed {
            // both changed while apart: the server's becomes active, ours is kept beside it
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let bak = path.with_file_name(format!("{}.local-{ts}.bak", path.file_name().and_then(|n| n.to_str()).unwrap_or("save")));
            if let Some(l) = &local
                && std::fs::write(&bak, l).is_ok()
            {
                kept += 1;
                tracing::warn!(file = %rf.file, backup = %bak.display(), "save sync: changed on both sides — the server's copy is active, this device's kept as a backup");
            }
        }
        if write_local(&path, &bytes).is_ok() {
            down += 1;
            st.files.insert(rf.file.clone(), Agreed { sha256: rf.sha256.clone(), stamp: stamp(&path) });
        }
    }
    save_state(&st);
    tracing::info!(server = %c.base, profile = %cfg.profile, downloaded = down, conflicts_kept = kept, ms = t0.elapsed().as_millis() as u64, "save sync: pulled at start");
}

/// One pass of the watcher: upload what changed since it was last agreed.
fn push_pass(c: &ServerConn, st: &mut State) -> bool {
    let mut changed = false;
    for (file, path) in local_files() {
        let s = stamp(&path);
        let agreed = st.files.get(&file).cloned();
        if agreed.as_ref().is_some_and(|a| a.stamp == s) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let local_sha = sha(&bytes);
        if agreed.as_ref().is_some_and(|a| a.sha256 == local_sha) {
            st.files.insert(file, Agreed { sha256: local_sha, stamp: s });
            changed = true;
            continue;
        }
        let res = match c.upload(&file, &bytes, agreed.as_ref().map(|a| a.sha256.as_str())) {
            Err(Some(server_sha)) => {
                // another device saved since: the game being played here wins, the server keeps the other as a version
                tracing::warn!(file, "save sync: the server had another device's newer copy — kept there as a version; this device's is current");
                c.upload(&file, &bytes, Some(&server_sha))
            }
            r => r,
        };
        match res {
            Ok(sha) => {
                tracing::info!(file, "save sync: uploaded");
                st.files.insert(file, Agreed { sha256: sha, stamp: s });
                changed = true;
            }
            Err(_) => return changed, // offline: try again next pass
        }
    }
    changed
}

/// The background uploader (every 5 s; reconnects when the server comes back).
pub fn start_watcher() {
    let Some(cfg) = config() else { return };
    let _ = std::thread::Builder::new().name("ou-save-sync".into()).spawn(move || {
        let mut client: Option<ServerConn> = None;
        let mut st = load_state();
        loop {
            if client.is_none() {
                client = ServerConn::connect(&cfg, Duration::from_secs(5));
            }
            if let Some(c) = client.as_ref() {
                if push_pass(c, &mut st) {
                    save_state(&st);
                }
                // a failed pass drops the connection so the next one re-picks the base URL
                if c.agent.get(format!("{}/healthz", c.base)).call().map_or(true, |r| !r.status().is_success()) {
                    client = None;
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_hex_sha256() {
        assert_eq!(sha(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn address_candidates() {
        assert_eq!(candidates("tower.local"), vec!["http://tower.local:8642"]);
        assert_eq!(candidates("192.168.50.37:9000"), vec!["http://192.168.50.37:9000"]);
        assert_eq!(candidates("tower-1.tawny-taipan.ts.net"), vec!["https://tower-1.tawny-taipan.ts.net", "http://tower-1.tawny-taipan.ts.net:8642"]);
        assert_eq!(candidates("http://x:1/"), vec!["http://x:1"]);
        assert!(candidates("  ").is_empty());
    }
}
