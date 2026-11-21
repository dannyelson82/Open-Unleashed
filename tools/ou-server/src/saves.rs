//! Saves: every profile's save files, shared by the Apple TV and the Mac (`server-plan.md` phase 2).
//!
//! Layout under `/data/saves/<profile>/`: `current/<file>` (the live copy), `versions/<file>/<id>` (earlier
//! copies) with `versions/<file>/index.json`, and `meta.json` (per file: SHA-256, size, last writer, time).
//! A `PUT` must name the copy it replaces (`If-Match: "<sha256>"`); anything else is a conflict (412) and nothing
//! is overwritten — the client keeps both. Every write keeps the previous copy as a version: the last
//! `OU_SAVE_VERSIONS`, plus the newest copy of each of the last 30 days.

use crate::{Shared, error};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path as FsPath, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMeta {
    pub sha256: String,
    pub size: u64,
    pub writer: String,
    /// unix milliseconds
    pub time: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub files: BTreeMap<String, FileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Version {
    pub id: String,
    #[serde(flatten)]
    pub meta: FileMeta,
}

pub struct Saves {
    pub root: PathBuf,
    pub keep: usize,
    /// one writer at a time (home use: a handful of devices)
    lock: tokio::sync::Mutex<()>,
}

/// Profile names: what the game allows for a player name, file-system safe.
pub fn profile_ok(p: &str) -> bool {
    !p.is_empty() && p.len() <= 64 && !p.starts_with('.') && p.chars().all(|c| c.is_ascii_alphanumeric() || " _-.'".contains(c))
}

/// The files the save service keeps: `SaveData/*.sav`, `SaveData/nfs5.trk`, `SaveData/*.json`, and the port's
/// `career.json`, `extras.json`, `settings.json`. Replays and photos stay on the device.
pub fn file_ok(f: &str) -> bool {
    if !ou_manifest::safe_rel_path(f) || f.len() > 128 {
        return false;
    }
    let lower = f.to_ascii_lowercase();
    match lower.split_once('/') {
        None => matches!(lower.as_str(), "career.json" | "extras.json" | "settings.json"),
        Some(("savedata", name)) => !name.contains('/') && (name.ends_with(".sav") || name == "nfs5.trk" || name.ends_with(".json")),
        _ => false,
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn etag(sha: &str) -> String {
    format!("\"{sha}\"")
}

/// `versions/<file>` with `/` → `__` (one folder per file).
fn version_dir(profile_dir: &FsPath, file: &str) -> PathBuf {
    profile_dir.join("versions").join(file.replace('/', "__"))
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(p: &FsPath) -> T {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Write through a temporary file + rename, so a crash never leaves half a save.
fn write_atomic(p: &FsPath, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_file_name(format!(".{}.tmp", p.file_name().and_then(|n| n.to_str()).unwrap_or("f")));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, p)
}

/// Which versions to keep: the newest `keep`, plus the newest of each day for 30 days.
pub fn retain(mut versions: Vec<Version>, keep: usize, now: u64) -> (Vec<Version>, Vec<Version>) {
    versions.sort_by(|a, b| b.meta.time.cmp(&a.meta.time)); // newest first
    let day = |t: u64| t / 86_400_000;
    let mut seen_days = std::collections::BTreeSet::new();
    let (mut kept, mut dropped) = (Vec::new(), Vec::new());
    for (i, v) in versions.into_iter().enumerate() {
        let recent_day = day(now).saturating_sub(day(v.meta.time)) < 30;
        let first_of_day = seen_days.insert(day(v.meta.time));
        if i < keep || (recent_day && first_of_day) {
            kept.push(v);
        } else {
            dropped.push(v);
        }
    }
    (kept, dropped)
}

impl Saves {
    pub fn new(root: PathBuf, keep: usize) -> Self {
        Self { root, keep, lock: tokio::sync::Mutex::new(()) }
    }

    fn dir(&self, profile: &str) -> PathBuf {
        self.root.join(profile)
    }

    pub fn profiles(&self) -> Vec<(String, Meta)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.root) {
            for e in rd.flatten() {
                if let Some(n) = e.file_name().to_str()
                    && profile_ok(n)
                    && e.path().is_dir()
                {
                    out.push((n.to_string(), read_json(&e.path().join("meta.json"))));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn meta(&self, profile: &str) -> Meta {
        read_json(&self.dir(profile).join("meta.json"))
    }

    pub fn versions(&self, profile: &str, file: &str) -> Vec<Version> {
        let mut v: Vec<Version> = read_json(&version_dir(&self.dir(profile), file).join("index.json"));
        v.sort_by(|a, b| b.meta.time.cmp(&a.meta.time));
        v
    }

    /// Keep the current copy of `file` as a version (called before it is replaced).
    fn push_version(&self, pdir: &FsPath, file: &str, cur: &FileMeta) -> std::io::Result<()> {
        let vdir = version_dir(pdir, file);
        let id = format!("{}-{}", cur.time, &cur.sha256[..12.min(cur.sha256.len())]);
        std::fs::create_dir_all(&vdir)?;
        std::fs::copy(pdir.join("current").join(file), vdir.join(&id))?;
        let mut idx: Vec<Version> = read_json(&vdir.join("index.json"));
        idx.retain(|v| v.id != id);
        idx.push(Version { id, meta: cur.clone() });
        let (kept, dropped) = retain(idx, self.keep, now_ms());
        for d in dropped {
            let _ = std::fs::remove_file(vdir.join(&d.id));
        }
        write_atomic(&vdir.join("index.json"), &serde_json::to_vec_pretty(&kept).unwrap())
    }

    /// Replace the current copy (the caller checked `If-Match`).
    fn store(&self, profile: &str, file: &str, bytes: &[u8], writer: &str) -> std::io::Result<FileMeta> {
        let pdir = self.dir(profile);
        let mut meta: Meta = read_json(&pdir.join("meta.json"));
        if let Some(cur) = meta.files.get(file).cloned()
            && pdir.join("current").join(file).is_file()
        {
            self.push_version(&pdir, file, &cur)?;
        }
        write_atomic(&pdir.join("current").join(file), bytes)?;
        let fm = FileMeta { sha256: ou_manifest::sha256_hex(bytes), size: bytes.len() as u64, writer: writer.to_string(), time: now_ms() };
        meta.files.insert(file.to_string(), fm.clone());
        write_atomic(&pdir.join("meta.json"), &serde_json::to_vec_pretty(&meta).unwrap())?;
        Ok(fm)
    }
}

pub fn routes(max_bytes: usize) -> Router<Shared> {
    Router::new()
        .route("/v1/saves", get(list_profiles))
        .route("/v1/saves/{profile}", get(list_files))
        .route("/v1/saves/{profile}/files/{*file}", get(get_file).put(put_file).layer(DefaultBodyLimit::max(max_bytes)))
        .route("/v1/saves/{profile}/versions/{*file}", get(list_versions))
        .route("/v1/saves/{profile}/version/{version}/{*file}", get(get_version))
        .route("/v1/saves/{profile}/restore/{version}/{*file}", post(restore))
}

fn check(profile: &str, file: Option<&str>) -> Option<Response> {
    if !profile_ok(profile) {
        return Some(error(StatusCode::BAD_REQUEST, "bad profile name"));
    }
    if let Some(f) = file
        && !file_ok(f)
    {
        return Some(error(StatusCode::BAD_REQUEST, "not a save file this server keeps (SaveData/*.sav, SaveData/nfs5.trk, SaveData/*.json, career.json, extras.json, settings.json)"));
    }
    None
}

async fn list_profiles(State(st): State<Shared>) -> Response {
    let list: Vec<_> = st
        .saves
        .profiles()
        .into_iter()
        .map(|(name, m)| {
            let last = m.files.values().map(|f| f.time).max().unwrap_or(0);
            serde_json::json!({ "profile": name, "files": m.files.len(), "last_write": last })
        })
        .collect();
    axum::Json(serde_json::json!({ "profiles": list })).into_response()
}

async fn list_files(State(st): State<Shared>, Path(profile): Path<String>) -> Response {
    if let Some(r) = check(&profile, None) {
        return r;
    }
    let m = st.saves.meta(&profile);
    let files: Vec<_> = m
        .files
        .iter()
        .map(|(f, fm)| serde_json::json!({ "file": f, "size": fm.size, "sha256": fm.sha256, "etag": etag(&fm.sha256), "writer": fm.writer, "time": fm.time }))
        .collect();
    axum::Json(serde_json::json!({ "profile": profile, "files": files })).into_response()
}

fn file_response(bytes: Vec<u8>, fm: &FileMeta) -> Response {
    let mut h = HeaderMap::new();
    h.insert(header::ETAG, HeaderValue::from_str(&etag(&fm.sha256)).unwrap());
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    h.insert("x-ou-writer", HeaderValue::from_str(&fm.writer).unwrap_or(HeaderValue::from_static("")));
    h.insert("x-ou-time", HeaderValue::from(fm.time));
    (StatusCode::OK, h, bytes).into_response()
}

async fn get_file(State(st): State<Shared>, Path((profile, file)): Path<(String, String)>) -> Response {
    if let Some(r) = check(&profile, Some(&file)) {
        return r;
    }
    let Some(fm) = st.saves.meta(&profile).files.get(&file).cloned() else { return error(StatusCode::NOT_FOUND, "no such save file") };
    match tokio::fs::read(st.saves.dir(&profile).join("current").join(&file)).await {
        Ok(b) => file_response(b, &fm),
        Err(_) => error(StatusCode::NOT_FOUND, "no such save file"),
    }
}

/// `PUT` with `If-Match: "<sha256 of the copy this device last saw>"` (or `If-None-Match: *` / no header for a
/// new file). 412 + the server's current ETag on a conflict; same bytes as now = 200 without a new version.
async fn put_file(State(st): State<Shared>, Path((profile, file)): Path<(String, String)>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(r) = check(&profile, Some(&file)) {
        return r;
    }
    let writer: String = headers.get("x-ou-device").and_then(|v| v.to_str().ok()).unwrap_or("unknown").chars().take(64).collect();
    let if_match = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()).map(str::trim);
    let _guard = st.saves.lock.lock().await;
    let cur = st.saves.meta(&profile).files.get(&file).cloned();
    let new_sha = ou_manifest::sha256_hex(&body);
    let conflict = |cur: Option<&FileMeta>| {
        let mut h = HeaderMap::new();
        if let Some(c) = cur {
            h.insert(header::ETAG, HeaderValue::from_str(&etag(&c.sha256)).unwrap());
        }
        let body = serde_json::json!({ "error": "conflict: the server has a different copy (kept; nothing overwritten)", "current": cur });
        (StatusCode::PRECONDITION_FAILED, h, axum::Json(body)).into_response()
    };
    match (&cur, if_match) {
        (Some(c), _) if c.sha256 == new_sha => {
            return (StatusCode::OK, [(header::ETAG, etag(&c.sha256))], axum::Json(serde_json::json!({ "unchanged": true, "file": c }))).into_response();
        }
        (Some(c), Some(m)) if m == "*" || m == etag(&c.sha256) => {}
        (Some(c), _) => return conflict(Some(c)),
        (None, None) | (None, Some("*")) => {}
        (None, Some(_)) => return conflict(None),
    }
    let saves = &st.saves;
    let res = tokio::task::block_in_place(|| saves.store(&profile, &file, &body, &writer));
    match res {
        Ok(fm) => {
            tracing::info!(profile, file, writer, size = fm.size, "save stored");
            let status = if cur.is_some() { StatusCode::OK } else { StatusCode::CREATED };
            (status, [(header::ETAG, etag(&fm.sha256))], axum::Json(serde_json::json!({ "file": fm }))).into_response()
        }
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, format!("save not stored: {e}")),
    }
}

async fn list_versions(State(st): State<Shared>, Path((profile, file)): Path<(String, String)>) -> Response {
    if let Some(r) = check(&profile, Some(&file)) {
        return r;
    }
    axum::Json(serde_json::json!({ "profile": profile, "file": file, "current": st.saves.meta(&profile).files.get(&file), "versions": st.saves.versions(&profile, &file) })).into_response()
}

fn version_ok(v: &str) -> bool {
    !v.is_empty() && v.len() <= 40 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

async fn get_version(State(st): State<Shared>, Path((profile, version, file)): Path<(String, String, String)>) -> Response {
    if let Some(r) = check(&profile, Some(&file)) {
        return r;
    }
    let Some(v) = st.saves.versions(&profile, &file).into_iter().find(|v| v.id == version).filter(|_| version_ok(&version)) else {
        return error(StatusCode::NOT_FOUND, "no such version");
    };
    match tokio::fs::read(version_dir(&st.saves.dir(&profile), &file).join(&v.id)).await {
        Ok(b) => file_response(b, &v.meta),
        Err(_) => error(StatusCode::NOT_FOUND, "version file missing"),
    }
}

/// Make a version current again (the current copy becomes a version itself — nothing is lost).
async fn restore(State(st): State<Shared>, Path((profile, version, file)): Path<(String, String, String)>, headers: HeaderMap) -> Response {
    if let Some(r) = check(&profile, Some(&file)) {
        return r;
    }
    let _guard = st.saves.lock.lock().await;
    let Some(v) = st.saves.versions(&profile, &file).into_iter().find(|v| v.id == version).filter(|_| version_ok(&version)) else {
        return error(StatusCode::NOT_FOUND, "no such version");
    };
    let Ok(bytes) = std::fs::read(version_dir(&st.saves.dir(&profile), &file).join(&v.id)) else { return error(StatusCode::NOT_FOUND, "version file missing") };
    let writer = format!("restore of {} by {}", v.id, headers.get("x-ou-device").and_then(|h| h.to_str().ok()).unwrap_or("status page"));
    let saves = &st.saves;
    match tokio::task::block_in_place(|| saves.store(&profile, &file, &bytes, &writer)) {
        Ok(fm) => {
            tracing::info!(profile, file, version, "save restored");
            // a browser form (status page) goes back to the page
            if headers.get(header::ACCEPT).and_then(|a| a.to_str().ok()).is_some_and(|a| a.contains("text/html")) {
                // back to the page it came from (keeps its `?token=`), else the status page
                let back = headers.get(header::REFERER).and_then(|r| r.to_str().ok()).and_then(|r| r.find("/?").map(|i| r[i..].to_string())).unwrap_or_else(|| "/".into());
                return (StatusCode::SEE_OTHER, [(header::LOCATION, back)]).into_response();
            }
            (StatusCode::OK, [(header::ETAG, etag(&fm.sha256))], axum::Json(serde_json::json!({ "file": fm }))).into_response()
        }
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, format!("restore failed: {e}")),
    }
}
