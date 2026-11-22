//! Game data sets for any number of games. The games root (`/games`) holds one folder per game (`porsche/`,
//! later `nfsse/`, …; adding a game = adding a folder), each with:
//!
//! * `original/` → set `<game>/original` (Porsche: `GameData/`, `FEData/` and the top-level files the engine
//!   reads; any other game: every file)
//! * `mods/<name>/` → set `<game>/mod-<name>`, overlaid on that game's original
//! * `hd/` → set `<game>/hd`
//!
//! Manifests are built at start-up and rebuilt when the folders change; hashes are cached in `/data/cache` keyed
//! by size + modification time. Only files a manifest lists are served.

use crate::{Shared, error};
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use nfs_haven_manifest::{HashCache, Manifest, Origin, SetKind};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// One built data set.
pub struct Built {
    pub manifest: Manifest,
    pub json: Arc<String>,
    /// manifest path → file on disk
    pub sources: BTreeMap<String, PathBuf>,
    pub kind: &'static str,
    /// the game it belongs to (`porsche`, …)
    pub game: String,
}

#[derive(Default)]
pub struct Sets {
    pub sets: BTreeMap<String, Arc<Built>>,
    pub building: bool,
    pub error: Option<String>,
    pub built_at: Option<std::time::SystemTime>,
}

#[derive(Default)]
pub struct Data {
    pub inner: RwLock<Sets>,
    /// look at the folders now (an import just published a set)
    pub kick: tokio::sync::Notify,
}

impl Data {
    /// The games that have at least one set.
    pub fn game_ids(&self) -> Vec<String> {
        let mut g: Vec<String> = self.inner.read().unwrap().sets.values().map(|b| b.game.clone()).collect();
        g.dedup();
        g
    }
    pub fn set_ids(&self) -> Vec<String> {
        self.inner.read().unwrap().sets.keys().cloned().collect()
    }
    pub fn get(&self, id: &str) -> Option<Arc<Built>> {
        self.inner.read().unwrap().sets.get(id).cloned()
    }
    pub fn state_text(&self) -> String {
        let s = self.inner.read().unwrap();
        if s.building {
            "building".into()
        } else if let Some(e) = &s.error {
            format!("error: {e}")
        } else if s.sets.is_empty() {
            "no data".into()
        } else {
            "ready".into()
        }
    }
}

/// A mod folder name usable in a set id: letters, digits, space, `_`, `-`, `.`, not hidden.
fn mod_name_ok(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && !n.starts_with('.') && n.chars().all(|c| c.is_ascii_alphanumeric() || " _-.".contains(c))
}

/// The games root's game folders (valid ids only), sorted.
fn games(cfg: &crate::Config) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(&cfg.games_dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| {
                    let id = e.file_name().to_str()?.to_string();
                    if nfs_haven_manifest::game_id_ok(&id) {
                        Some((id, e.path()))
                    } else {
                        if !id.starts_with('.') {
                            tracing::warn!(folder = id, "game folder skipped: use lower-case letters, digits and - only");
                        }
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// A game's mod folders (`mods/<name>/`), sorted.
fn mod_dirs(game_dir: &std::path::Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(game_dir.join("mods"))
        .map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.path()))).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// A cheap fingerprint of every folder (paths, sizes, mtimes) to notice changes without hashing.
fn fingerprint(cfg: &crate::Config) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut walk = |root: &std::path::Path, kind: SetKind| {
        root.display().to_string().hash(&mut h);
        for rel in nfs_haven_manifest::list_files(root, kind).unwrap_or_default() {
            rel.hash(&mut h);
            if let Ok(md) = std::fs::metadata(root.join(&rel)) {
                md.len().hash(&mut h);
                md.modified().ok().hash(&mut h);
            }
        }
    };
    for (id, dir) in games(cfg) {
        let (orig_kind, mod_kind) = nfs_haven_manifest::kinds_for(&id);
        walk(&dir.join("original"), orig_kind);
        for (_, d) in mod_dirs(&dir) {
            walk(&d, mod_kind);
        }
        walk(&dir.join("hd"), SetKind::Hd);
    }
    h.finish()
}

/// Build every game's sets (blocking: hashing).
fn build_all(cfg: &crate::Config) -> Result<BTreeMap<String, Arc<Built>>, String> {
    let cache_path = cfg.data_dir.join("cache").join("hashes.json");
    let mut cache = HashCache::load(&cache_path);
    let mut sets = BTreeMap::new();
    let all = games(cfg);
    if all.is_empty() {
        tracing::warn!(dir = %cfg.games_dir.display(), "no game folders (e.g. porsche/original/) in the games folder: no data sets");
    }
    for (game, dir) in all {
        build_game(&game, &dir, &mut cache, &mut sets)?;
    }
    if let Err(e) = cache.save(&cache_path) {
        tracing::warn!(error = %e, "hash cache not saved");
    }
    Ok(sets)
}

/// One game's sets: `<game>/original`, `<game>/mod-<name>` (each overlaid on the original), `<game>/hd`.
fn build_game(game: &str, dir: &std::path::Path, cache: &mut HashCache, sets: &mut BTreeMap<String, Arc<Built>>) -> Result<(), String> {
    let (orig_kind, mod_kind) = nfs_haven_manifest::kinds_for(game);
    let orig_dir = dir.join("original");
    if nfs_haven_manifest::original_ok(game, &orig_dir) {
        let id = format!("{game}/original");
        let original = nfs_haven_manifest::build(&orig_dir, orig_kind, &id, cache).map_err(|e| format!("{}: {e}", orig_dir.display()))?;
        let sources = original.files.iter().map(|f| (f.path.clone(), orig_dir.join(&f.path))).collect();
        tracing::info!(set = id, files = original.files.len(), mb = original.total_size / 1_000_000, "set original");
        for (name, d) in mod_dirs(dir) {
            if !mod_name_ok(&name) {
                tracing::warn!(game, name, "mod folder skipped: use letters, digits, space, _ - . only");
                continue;
            }
            let id = format!("{game}/mod-{name}");
            let m = match nfs_haven_manifest::build(&d, mod_kind, &id, cache) {
                Ok(m) if !m.files.is_empty() => m,
                Ok(_) => {
                    tracing::warn!(game, name, "mod folder has no files the game uses: skipped");
                    continue;
                }
                Err(e) => {
                    tracing::warn!(game, name, error = %e, "mod folder unreadable: skipped");
                    continue;
                }
            };
            let (merged, origins) = nfs_haven_manifest::overlay(&original, &m, &id);
            let sources = origins.into_iter().map(|(path, (o, src))| (path, if o == Origin::Overlay { d.join(src) } else { orig_dir.join(src) })).collect();
            tracing::info!(set = id, replaced_or_added = m.files.len(), files = merged.files.len(), "set mod");
            sets.insert(id, Arc::new(Built { json: Arc::new(merged.to_json()), manifest: merged, sources, kind: "mod", game: game.to_string() }));
        }
        sets.insert(id, Arc::new(Built { json: Arc::new(original.to_json()), manifest: original, sources, kind: "original", game: game.to_string() }));
    } else {
        tracing::warn!(game, dir = %orig_dir.display(), "no usable original/ folder: this game has no data sets");
    }
    let hd_dir = dir.join("hd");
    if hd_dir.is_dir() {
        let id = format!("{game}/hd");
        match nfs_haven_manifest::build(&hd_dir, SetKind::Hd, &id, cache) {
            Ok(m) if !m.files.is_empty() => {
                let sources = m.files.iter().map(|f| (f.path.clone(), hd_dir.join(&f.path))).collect();
                tracing::info!(set = id, files = m.files.len(), mb = m.total_size / 1_000_000, "set hd");
                sets.insert(id, Arc::new(Built { json: Arc::new(m.to_json()), manifest: m, sources, kind: "hd", game: game.to_string() }));
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(game, error = %e, "hd folder unreadable"),
        }
    }
    Ok(())
}

/// Build now, synchronously (tests).
#[cfg(test)]
pub fn rebuild_now(st: &Shared) {
    let sets = build_all(&st.cfg).expect("build");
    st.data.inner.write().unwrap().sets = sets;
}

/// Build at start-up, then rebuild whenever the folders' fingerprint changes.
pub fn spawn_scanner(st: Shared) {
    tokio::spawn(async move {
        let mut last = None;
        loop {
            let cfg = st.cfg.clone();
            let fp = tokio::task::spawn_blocking(move || fingerprint(&cfg)).await.ok();
            if fp != last {
                st.data.inner.write().unwrap().building = true;
                let cfg = st.cfg.clone();
                let t0 = std::time::Instant::now();
                let res = tokio::task::spawn_blocking(move || build_all(&cfg)).await.unwrap_or_else(|e| Err(e.to_string()));
                let mut s = st.data.inner.write().unwrap();
                s.building = false;
                match res {
                    Ok(sets) => {
                        s.sets = sets;
                        s.error = None;
                        s.built_at = Some(std::time::SystemTime::now());
                        tracing::info!(secs = t0.elapsed().as_secs_f32(), sets = s.sets.len(), "manifests built");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "manifest build failed");
                        s.error = Some(e);
                    }
                }
                last = fp;
            }
            tokio::select! {
                _ = tokio::time::sleep(st.cfg.rescan) => {}
                _ = st.data.kick.notified() => {}
            }
        }
    });
}

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/v1/data/sets", get(sets))
        .route("/v1/data/{game}/{set}/manifest", get(manifest))
        .route("/v1/data/{game}/{set}/file/{*path}", get(file))
        .route("/v1/data/{game}/{file}", get(crate::bundle::tar))
}

async fn sets(State(st): State<Shared>) -> Response {
    let s = st.data.inner.read().unwrap();
    let list: Vec<_> = s
        .sets
        .iter()
        .map(|(id, b)| {
            serde_json::json!({
                "id": id,
                "game": b.game,
                "kind": b.kind,
                "files": b.manifest.files.len(),
                "size": b.manifest.total_size,
            })
        })
        .collect();
    axum::Json(serde_json::json!({ "state": if s.building { "building" } else { "ready" }, "sets": list })).into_response()
}

async fn manifest(State(st): State<Shared>, Path((game, set)): Path<(String, String)>) -> Response {
    let set = format!("{game}/{set}");
    match st.data.get(&set) {
        Some(b) => ([(header::CONTENT_TYPE, "application/json")], b.json.as_str().to_string()).into_response(),
        None => error(StatusCode::NOT_FOUND, format!("no data set {set:?} (state: {})", st.data.state_text())),
    }
}

/// `bytes=a-b`, `bytes=a-`, `bytes=-n` → inclusive range within `len`; `Err` = unsatisfiable.
pub fn parse_range(v: &str, len: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = v.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None; // multipart ranges: serve the whole file
    }
    let (a, b) = spec.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    if len == 0 {
        return Some(Err(()));
    }
    let r = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return Some(Err(()));
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
        if start >= len || start > end {
            return Some(Err(()));
        }
        (start, end)
    };
    Some(Ok(r))
}

async fn file(State(st): State<Shared>, Path((game, set, path)): Path<(String, String, String)>, headers: HeaderMap) -> Response {
    let set = format!("{game}/{set}");
    let Some(b) = st.data.get(&set) else { return error(StatusCode::NOT_FOUND, format!("no data set {set:?}")) };
    if !nfs_haven_manifest::safe_rel_path(&path) {
        return error(StatusCode::BAD_REQUEST, "bad path");
    }
    let (Some(entry), Some(src)) = (b.manifest.get(&path), b.sources.get(&path)) else {
        return error(StatusCode::NOT_FOUND, "not in the manifest");
    };
    let etag = format!("\"{}\"", entry.sha256);
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    let mut f = match tokio::fs::File::open(src).await {
        Ok(f) => f,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, format!("file unreadable: {e}")),
    };
    let len = f.metadata().await.map(|m| m.len()).unwrap_or(entry.size);
    if len != entry.size {
        // changed on disk since the manifest was built; the scanner will catch up
        return error(StatusCode::SERVICE_UNAVAILABLE, "file changed since the manifest was built; retry shortly");
    }
    // `If-Range` with another ETag: the whole file
    let range_ok = headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()).is_none_or(|v| v == etag);
    let range = if range_ok { headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| parse_range(v, len)) } else { None };
    let mut h = HeaderMap::new();
    h.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    h.insert("x-content-sha256", HeaderValue::from_str(&entry.sha256).unwrap());
    match range {
        Some(Err(())) => {
            h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{len}")).unwrap());
            (StatusCode::RANGE_NOT_SATISFIABLE, h).into_response()
        }
        Some(Ok((start, end))) => {
            if f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                return error(StatusCode::INTERNAL_SERVER_ERROR, "seek failed");
            }
            let n = end - start + 1;
            h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {start}-{end}/{len}")).unwrap());
            h.insert(header::CONTENT_LENGTH, HeaderValue::from(n));
            (StatusCode::PARTIAL_CONTENT, h, Body::from_stream(tokio_util::io::ReaderStream::new(f.take(n)))).into_response()
        }
        None => {
            h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
            (StatusCode::OK, h, Body::from_stream(tokio_util::io::ReaderStream::new(f))).into_response()
        }
    }
}
