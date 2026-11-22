//! Lobby (`server-plan.md` phase 3): hosts register their network game with every address they have (LAN and
//! Tailscale 100.x), heartbeat every few seconds and remove it when the session closes; joiners list the open
//! games. Race traffic stays player-to-player (UDP 9442) — the server only says where to find each other.
//! Each game's addresses are ordered for the asking device: a tailnet requester gets 100.x addresses first, a LAN
//! requester the LAN ones.

use crate::{Shared, error};
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What a host registers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInfo {
    pub name: String,
    #[serde(default)]
    pub host_player: String,
    #[serde(default)]
    pub track: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub laps: u32,
    #[serde(default)]
    pub players: u32,
    #[serde(default = "default_max")]
    pub max_players: u32,
    /// the game's `version.txt` (a host refuses other versions)
    #[serde(default)]
    pub version: String,
    /// every address of the host (LAN and tailnet), without the port
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub started: bool,
    #[serde(default)]
    pub assists: bool,
}

fn default_max() -> u32 {
    8
}
fn default_port() -> u16 {
    9442
}

/// A heartbeat's optional updates.
#[derive(Debug, Default, Deserialize)]
pub struct Heartbeat {
    pub players: Option<u32>,
    pub started: Option<bool>,
    pub track: Option<String>,
}

struct Entry {
    info: GameInfo,
    registered: SystemTime,
    seen: Instant,
}

pub struct Lobby {
    ttl: Duration,
    games: Mutex<HashMap<String, Entry>>,
    counter: std::sync::atomic::AtomicU64,
}

/// Tailscale's CGNAT range 100.64.0.0/10 (and its IPv6 fd7a:115c:a1e0::/48).
pub fn is_tailnet(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.octets()[0] == 100 && (v.octets()[1] & 0xC0) == 64,
        IpAddr::V6(v) => v.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
    }
}

fn is_lan(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_link_local(),
        IpAddr::V6(v) => (v.segments()[0] & 0xfe00) == 0xfc00 && !is_tailnet(ip),
    }
}

/// Addresses ordered for a requester: tailnet first for a tailnet requester, LAN first otherwise; then the rest.
pub fn order_addresses(addrs: &[String], tailnet_requester: bool) -> Vec<String> {
    let rank = |a: &String| {
        let ip: Option<IpAddr> = a.parse().ok();
        let (t, l) = ip.map_or((false, false), |ip| (is_tailnet(&ip), is_lan(&ip)));
        match (tailnet_requester, t, l) {
            (true, true, _) | (false, _, true) => 0,
            (true, _, true) | (false, true, _) => 1,
            _ => 2,
        }
    };
    let mut v: Vec<String> = addrs.to_vec();
    v.sort_by_key(rank); // stable: the host's own order within a rank
    v.dedup();
    v
}

impl Lobby {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, games: Mutex::new(HashMap::new()), counter: Default::default() }
    }

    fn new_id(&self) -> String {
        let n = self.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let t = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        nfs_haven_manifest::sha256_hex(format!("{t}:{n}:{}", std::process::id()).as_bytes())[..16].to_string()
    }

    pub fn sweep(&self) {
        let ttl = self.ttl;
        self.games.lock().unwrap().retain(|id, g| {
            let keep = g.seen.elapsed() < ttl;
            if !keep {
                tracing::info!(id, name = g.info.name, "lobby game expired");
            }
            keep
        });
    }

    /// (id, info, registered) of every open game.
    pub fn list(&self) -> Vec<(String, GameInfo, SystemTime)> {
        self.sweep();
        let mut v: Vec<_> = self.games.lock().unwrap().iter().map(|(id, g)| (id.clone(), g.info.clone(), g.registered)).collect();
        v.sort_by(|a, b| a.2.cmp(&b.2));
        v
    }
}

pub fn spawn_sweeper(st: Shared) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            st.lobby.sweep();
        }
    });
}

pub fn routes() -> Router<Shared> {
    Router::new().route("/v1/lobby/games", get(list).post(register)).route("/v1/lobby/games/{id}", put(heartbeat).delete(remove))
}

#[derive(Deserialize)]
struct ListQuery {
    /// `tailnet` / `lan`: how the client reached the server (it knows; behind Docker NAT the server may not)
    via: Option<String>,
}

async fn list(State(st): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, Query(q): Query<ListQuery>) -> Response {
    let tailnet = match q.via.as_deref() {
        Some("tailnet") => true,
        Some("lan") => false,
        _ => is_tailnet(&peer.ip()),
    };
    let games: Vec<_> = st
        .lobby
        .list()
        .into_iter()
        .map(|(id, g, reg)| {
            let addrs = order_addresses(&g.addresses, tailnet);
            serde_json::json!({
                "id": id, "name": g.name, "host_player": g.host_player, "track": g.track, "mode": g.mode, "laps": g.laps,
                "players": g.players, "max_players": g.max_players, "version": g.version, "started": g.started,
                "assists": g.assists, "port": g.port, "addresses": addrs,
                "registered": reg.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            })
        })
        .collect();
    Json(serde_json::json!({ "games": games })).into_response()
}

async fn register(State(st): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, Json(mut info): Json<GameInfo>) -> Response {
    info.name = info.name.chars().take(64).collect();
    if info.name.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "name required");
    }
    // keep only real IP addresses; add the address the host was seen from
    info.addresses.retain(|a| a.parse::<IpAddr>().is_ok());
    let seen_from = peer.ip().to_string();
    if !peer.ip().is_loopback() && !info.addresses.contains(&seen_from) {
        info.addresses.push(seen_from);
    }
    info.addresses.truncate(16);
    let id = st.lobby.new_id();
    tracing::info!(id, name = info.name, addresses = ?info.addresses, "lobby game registered");
    st.lobby.games.lock().unwrap().insert(id.clone(), Entry { info, registered: SystemTime::now(), seen: Instant::now() });
    (StatusCode::CREATED, Json(serde_json::json!({ "id": id, "heartbeat_secs": 5, "ttl_secs": st.cfg.lobby_ttl.as_secs() }))).into_response()
}

async fn heartbeat(State(st): State<Shared>, Path(id): Path<String>, body: Option<Json<Heartbeat>>) -> Response {
    let mut games = st.lobby.games.lock().unwrap();
    let Some(g) = games.get_mut(&id) else { return error(StatusCode::NOT_FOUND, "no such game (expired? register again)") };
    g.seen = Instant::now();
    if let Some(Json(h)) = body {
        if let Some(p) = h.players {
            g.info.players = p;
        }
        if let Some(s) = h.started {
            g.info.started = s;
        }
        if let Some(t) = h.track {
            g.info.track = t.chars().take(64).collect();
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn remove(State(st): State<Shared>, Path(id): Path<String>) -> Response {
    match st.lobby.games.lock().unwrap().remove(&id) {
        Some(g) => {
            tracing::info!(id, name = g.info.name, "lobby game closed");
            StatusCode::NO_CONTENT.into_response()
        }
        None => error(StatusCode::NOT_FOUND, "no such game"),
    }
}
