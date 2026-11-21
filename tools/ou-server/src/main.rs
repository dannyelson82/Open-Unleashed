//! **ou-server** — the OpenUnleashed server (`server-plan.md`, `docs/ou-server.md`): one container on the home
//! server that the Apple TV and Mac builds point at, on the LAN or over Tailscale.
//!
//! * game data: the original files, mod sets overlaid on them, the HD pack — manifests + files with `Range`
//! * saves: per-profile save files with `If-Match` conflict detection, version history and restore
//! * lobby: hosted network games, so they can be joined over Tailscale where LAN broadcast can't reach
//! * a status page (`/`) and `/healthz`
//!
//! LAN/tailnet only — never port-forward it to the internet. GPL-3.0-or-later.

mod data;
mod lobby;
mod saves;
mod status;

use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Settings from the environment (`docs/ou-server.md`).
#[derive(Debug, Clone)]
pub struct Config {
    pub token: String,
    pub port: u16,
    pub name: String,
    pub save_versions: usize,
    pub lobby_ttl: Duration,
    pub game_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub hd_dir: PathBuf,
    pub data_dir: PathBuf,
    pub rescan: Duration,
    pub save_max_bytes: usize,
}

impl Config {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let num = |k: &str, d: u64| var(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
        Self {
            token: var("OU_TOKEN").unwrap_or_default(),
            port: num("OU_PORT", 8642) as u16,
            name: var("OU_NAME").unwrap_or_else(|| "OpenUnleashed server".into()),
            save_versions: num("OU_SAVE_VERSIONS", 20).max(1) as usize,
            lobby_ttl: Duration::from_secs(num("OU_LOBBY_TTL", 15).max(5)),
            game_dir: var("OU_GAME_DIR").unwrap_or_else(|| "/game".into()).into(),
            mods_dir: var("OU_MODS_DIR").unwrap_or_else(|| "/mods".into()).into(),
            hd_dir: var("OU_HD_DIR").unwrap_or_else(|| "/hd".into()).into(),
            data_dir: var("OU_DATA_DIR").unwrap_or_else(|| "/data".into()).into(),
            rescan: Duration::from_secs(num("OU_RESCAN_SECS", 60).max(5)),
            save_max_bytes: num("OU_SAVE_MAX_BYTES", 8 << 20) as usize,
        }
    }
}

/// A device seen recently (status page).
#[derive(Debug, Clone)]
pub struct Seen {
    pub addr: String,
    pub at: SystemTime,
    pub last: String,
}

pub struct AppState {
    pub cfg: Config,
    pub data: data::Data,
    pub saves: saves::Saves,
    pub lobby: lobby::Lobby,
    pub seen: Mutex<BTreeMap<String, Seen>>,
}

pub type Shared = Arc<AppState>;

/// JSON error body `{ "error": … }`.
pub fn error(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, axum::Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

/// Constant-time comparison of the shared token.
fn token_ok(cfg: &Config, given: Option<&str>) -> bool {
    if cfg.token.is_empty() {
        return true;
    }
    let Some(g) = given else { return false };
    let (a, b) = (cfg.token.as_bytes(), g.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `Authorization: Bearer <token>`, or `?token=` (status page in a browser).
fn given_token<'a>(headers: &'a HeaderMap, query: Option<&'a str>) -> Option<&'a str> {
    if let Some(v) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok())
        && let Some(t) = v.strip_prefix("Bearer ")
    {
        return Some(t.trim());
    }
    query?.split('&').find_map(|kv| kv.strip_prefix("token="))
}

async fn auth(State(st): State<Shared>, req: Request, next: Next) -> Response {
    if req.uri().path() == "/healthz" {
        return next.run(req).await;
    }
    if !token_ok(&st.cfg, given_token(req.headers(), req.uri().query())) {
        return error(StatusCode::UNAUTHORIZED, "missing or wrong token (Authorization: Bearer <OU_TOKEN>)");
    }
    // remember the device for the status page
    let device = req.headers().get("x-ou-device").and_then(|v| v.to_str().ok()).unwrap_or("").chars().take(64).collect::<String>();
    let addr = req.extensions().get::<axum::extract::ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string()).unwrap_or_default();
    let key = if device.is_empty() { addr.clone() } else { device };
    if let Ok(mut seen) = st.seen.lock() {
        seen.insert(key, Seen { addr, at: SystemTime::now(), last: format!("{} {}", req.method(), req.uri().path()) });
        if seen.len() > 200 {
            let oldest = seen.iter().min_by_key(|(_, s)| s.at).map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                seen.remove(&k);
            }
        }
    }
    next.run(req).await
}

async fn info(State(st): State<Shared>) -> Response {
    let sets = st.data.set_ids();
    axum::Json(serde_json::json!({
        "name": st.cfg.name,
        "server": "ou-server",
        "version": VERSION,
        "api": 1,
        "features": ["data", "saves", "lobby"],
        "data_sets": sets,
        "data_state": st.data.state_text(),
        "lobby": true,
        "lobby_ttl_secs": st.cfg.lobby_ttl.as_secs(),
    }))
    .into_response()
}

pub fn router(st: Shared) -> Router {
    Router::new()
        .route("/", get(status::page))
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/info", get(info))
        .merge(data::routes())
        .merge(saves::routes(st.cfg.save_max_bytes))
        .merge(lobby::routes())
        .layer(middleware::from_fn_with_state(st.clone(), auth))
        .with_state(st)
}

pub fn new_state(cfg: Config) -> Shared {
    let saves = saves::Saves::new(cfg.data_dir.join("saves"), cfg.save_versions);
    let lobby = lobby::Lobby::new(cfg.lobby_ttl);
    Arc::new(AppState { data: data::Data::default(), saves, lobby, seen: Mutex::new(BTreeMap::new()), cfg })
}

/// `ou-server --healthcheck`: the container's HEALTHCHECK (no curl in the image) — GET /healthz on the local port.
fn healthcheck() -> ! {
    use std::io::{Read, Write};
    let port = Config::from_env().port;
    let ok = (|| -> std::io::Result<bool> {
        let mut s = std::net::TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_secs(3))?;
        s.set_read_timeout(Some(Duration::from_secs(3)))?;
        s.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
        let mut buf = String::new();
        s.read_to_string(&mut buf)?;
        Ok(buf.starts_with("HTTP/1.1 200"))
    })()
    .unwrap_or(false);
    std::process::exit(if ok { 0 } else { 1 })
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        healthcheck();
    }
    use std::io::IsTerminal;
    tracing_subscriber::fmt().with_ansi(std::io::stdout().is_terminal()).with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let cfg = Config::from_env();
    if cfg.token.is_empty() {
        tracing::warn!("OU_TOKEN is not set: anyone on the network can use this server (set a token)");
    }
    tracing::info!(game = %cfg.game_dir.display(), mods = %cfg.mods_dir.display(), hd = %cfg.hd_dir.display(), data = %cfg.data_dir.display(), port = cfg.port, "ou-server {VERSION} starting");
    if let Err(e) = std::fs::create_dir_all(&cfg.data_dir) {
        tracing::error!(error = %e, dir = %cfg.data_dir.display(), "data folder not writable");
    }
    let st = new_state(cfg.clone());
    data::spawn_scanner(st.clone());
    lobby::spawn_sweeper(st.clone());
    let app = router(st);
    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, %addr, "cannot listen");
            std::process::exit(1);
        }
    };
    tracing::info!(%addr, "listening");
    let shutdown = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
            tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
        }
        #[cfg(not(unix))]
        let _ = ctrl_c.await;
        tracing::info!("shutting down");
    };
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(shutdown).await.expect("server");
}

#[cfg(test)]
mod tests;
