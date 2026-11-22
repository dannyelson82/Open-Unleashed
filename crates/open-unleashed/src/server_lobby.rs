//! **PLATFORM GLUE (multiplayer lobby, `server-plan.md` phase 3)** — network games through the OpenUnleashed
//! server, so they can be found over Tailscale (where LAN broadcast doesn't reach). Nothing without a server set.
//!
//! * **Hosting:** `CREATE` registers the session (name, host player, players, every local address with the
//!   well-known port) and heartbeats every 5 s; it is deleted when the session ends (and expires on the server
//!   after `OU_LOBBY_TTL` otherwise).
//! * **Joining:** while a games screen is open, the server's open games are fetched every 3 s and the original's
//!   `'w'` probe is also sent **unicast** to each game's best address (the server orders them: the tailnet one first
//!   for a tailnet requester). A host that answers appears in the original's games list like a LAN one — real
//!   name, version and ping — and JOIN takes the usual path (`parse_typed_address` / the join).

use crate::sync::{ServerConn, server_config};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The best address of each open game on the server (refreshed while wanted).
static GAMES: Mutex<Vec<SocketAddr>> = Mutex::new(Vec::new());
/// When a games screen last asked (unix ms); the fetcher stops when nobody has for 10 s.
static WANTED_AT: AtomicU64 = AtomicU64::new(0);
static FETCHER: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The games screen's probe: the server's games to probe unicast (empty without a server). Starts the fetcher.
pub fn probe_targets() -> Vec<SocketAddr> {
    let Some(cfg) = server_config() else { return Vec::new() };
    WANTED_AT.store(now_ms(), Ordering::Relaxed);
    if !FETCHER.swap(true, Ordering::AcqRel) {
        let _ = std::thread::Builder::new().name("ou-lobby-list".into()).spawn(move || {
            let mut conn: Option<ServerConn> = None;
            while now_ms().saturating_sub(WANTED_AT.load(Ordering::Relaxed)) < 10_000 {
                if conn.is_none() {
                    conn = ServerConn::connect(&cfg, Duration::from_secs(3));
                }
                if let Some(c) = conn.as_ref() {
                    match c.lobby_games() {
                        Some(list) => {
                            if let Ok(mut g) = GAMES.lock() {
                                *g = list;
                            }
                        }
                        None => conn = None,
                    }
                }
                std::thread::sleep(Duration::from_secs(3));
            }
            if let Ok(mut g) = GAMES.lock() {
                g.clear();
            }
            FETCHER.store(false, Ordering::Release);
        });
    }
    GAMES.lock().map(|g| g.clone()).unwrap_or_default()
}

/// A hosted session's registration; dropping it removes the game from the server.
pub struct Registration {
    stop: Arc<AtomicBool>,
    players: Arc<AtomicU32>,
    started: Arc<AtomicBool>,
}

impl Registration {
    /// Register `name` hosted by `who` on `port`; `None` without a server.
    pub fn register(name: &str, who: &str, port: u16) -> Option<Self> {
        let cfg = server_config()?;
        let stop = Arc::new(AtomicBool::new(false));
        let players = Arc::new(AtomicU32::new(1));
        let started = Arc::new(AtomicBool::new(false));
        let (s, p, st) = (stop.clone(), players.clone(), started.clone());
        let addresses: Vec<String> = nfs_net::socket::interfaces().into_iter().map(|(ip, _)| ip.to_string()).filter(|a| a != "127.0.0.1").collect();
        let body = serde_json::json!({ "name": name, "host_player": who, "players": 1, "max_players": 8, "addresses": addresses, "port": port });
        let _ = std::thread::Builder::new().name("ou-lobby-host".into()).spawn(move || {
            let mut conn: Option<ServerConn> = None;
            let mut id: Option<String> = None;
            let mut last = Instant::now() - Duration::from_secs(60);
            while !s.load(Ordering::Acquire) {
                if last.elapsed() >= Duration::from_secs(5) {
                    last = Instant::now();
                    if conn.is_none() {
                        conn = ServerConn::connect(&cfg, Duration::from_secs(3));
                    }
                    if let Some(c) = conn.as_ref() {
                        let ok = match id.as_deref() {
                            None => match c.lobby_register(&body) {
                                Some(new_id) => {
                                    tracing::info!(id = %new_id, server = %c.base(), "lobby: game registered");
                                    id = Some(new_id);
                                    true
                                }
                                None => false,
                            },
                            Some(gid) => match c.lobby_heartbeat(gid, p.load(Ordering::Relaxed), st.load(Ordering::Relaxed)) {
                                Some(true) => true,
                                // expired on the server (it was away too long): register again
                                Some(false) => {
                                    id = None;
                                    true
                                }
                                None => false,
                            },
                        };
                        if !ok {
                            conn = None;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            if let (Some(c), Some(gid)) = (conn.as_ref(), id.as_deref()) {
                c.lobby_delete(gid);
                tracing::info!(id = gid, "lobby: game removed");
            }
        });
        Some(Self { stop, players, started })
    }

    pub fn set_players(&self, n: u32) {
        self.players.store(n, Ordering::Relaxed);
    }

    pub fn set_started(&self, on: bool) {
        self.started.store(on, Ordering::Relaxed);
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
