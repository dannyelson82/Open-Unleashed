//! In-process tests of the HTTP API (no network).

use super::*;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ou-server-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn cfg(root: &std::path::Path, token: &str) -> Config {
    Config {
        token: token.into(),
        port: 0,
        name: "test".into(),
        save_versions: 3,
        lobby_ttl: Duration::from_millis(300),
        game_dir: root.join("game"),
        mods_dir: root.join("mods"),
        hd_dir: root.join("hd"),
        data_dir: root.join("data"),
        rescan: Duration::from_secs(60),
        save_max_bytes: 1 << 20,
    }
}

fn game(root: &std::path::Path) {
    let g = root.join("game");
    std::fs::create_dir_all(g.join("GameData/CarModel")).unwrap();
    std::fs::create_dir_all(g.join("FEData/Art")).unwrap();
    std::fs::write(g.join("GameData/CarModel/911.crp"), b"0123456789").unwrap();
    std::fs::write(g.join("FEData/Art/menu.fsh"), b"menu").unwrap();
    std::fs::write(g.join("version.txt"), b"3.2").unwrap();
    std::fs::write(g.join("Porsche.exe"), b"never served").unwrap();
}

async fn call(app: &Router, method: Method, uri: &str, headers: &[(&str, &str)], body: Vec<u8>, from: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut req = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut req = req.body(Body::from(body)).unwrap();
    req.extensions_mut().insert(ConnectInfo::<SocketAddr>(format!("{from}:50000").parse().unwrap()));
    let res = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    (parts.status, parts.headers, body.collect().await.unwrap().to_bytes().to_vec())
}

const AUTH: (&str, &str) = ("authorization", "Bearer s3cret");

#[tokio::test]
async fn auth_is_required_except_healthz() {
    let root = tmp("auth");
    let st = new_state(cfg(&root, "s3cret"));
    let app = router(st);
    assert_eq!(call(&app, Method::GET, "/healthz", &[], vec![], "192.168.1.2").await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::GET, "/v1/info", &[], vec![], "192.168.1.2").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, Method::GET, "/v1/info", &[("authorization", "Bearer nope")], vec![], "192.168.1.2").await.0, StatusCode::UNAUTHORIZED);
    let (s, _, b) = call(&app, Method::GET, "/v1/info", &[AUTH], vec![], "192.168.1.2").await;
    assert_eq!(s, StatusCode::OK);
    assert!(String::from_utf8(b).unwrap().contains("\"lobby\":true"));
    assert_eq!(call(&app, Method::GET, "/?token=s3cret", &[], vec![], "192.168.1.2").await.0, StatusCode::OK);
}

#[tokio::test]
async fn data_manifest_files_and_ranges() {
    let root = tmp("data");
    game(&root);
    let st = new_state(cfg(&root, "s3cret"));
    data::rebuild_now(&st);
    let app = router(st);
    let (s, _, b) = call(&app, Method::GET, "/v1/data/original/manifest", &[AUTH], vec![], "10.0.0.2").await;
    assert_eq!(s, StatusCode::OK);
    let m: ou_manifest::Manifest = serde_json::from_slice(&b).unwrap();
    assert_eq!(m.files.len(), 3, "GameData, FEData, version.txt — not Porsche.exe");
    // same bytes as make-manifest would write
    let mut c = ou_manifest::HashCache::default();
    assert_eq!(String::from_utf8(b).unwrap(), ou_manifest::build(&root.join("game"), ou_manifest::SetKind::Original, "original", &mut c).unwrap().to_json());
    let (s, h, b) = call(&app, Method::GET, "/v1/data/original/file/GameData/CarModel/911.crp", &[AUTH], vec![], "10.0.0.2").await;
    assert_eq!((s, b.as_slice()), (StatusCode::OK, &b"0123456789"[..]));
    assert_eq!(h["accept-ranges"], "bytes");
    let (s, h, b) = call(&app, Method::GET, "/v1/data/original/file/GameData/CarModel/911.crp", &[AUTH, ("range", "bytes=4-")], vec![], "10.0.0.2").await;
    assert_eq!((s, b.as_slice()), (StatusCode::PARTIAL_CONTENT, &b"456789"[..]));
    assert_eq!(h["content-range"], "bytes 4-9/10");
    let (s, _, b) = call(&app, Method::GET, "/v1/data/original/file/GameData/CarModel/911.crp", &[AUTH, ("range", "bytes=-3")], vec![], "10.0.0.2").await;
    assert_eq!((s, b.as_slice()), (StatusCode::PARTIAL_CONTENT, &b"789"[..]));
    assert_eq!(call(&app, Method::GET, "/v1/data/original/file/GameData/CarModel/911.crp", &[AUTH, ("range", "bytes=20-")], vec![], "10.0.0.2").await.0, StatusCode::RANGE_NOT_SATISFIABLE);
    // never anything outside the manifest
    assert_eq!(call(&app, Method::GET, "/v1/data/original/file/Porsche.exe", &[AUTH], vec![], "10.0.0.2").await.0, StatusCode::NOT_FOUND);
    assert_ne!(call(&app, Method::GET, "/v1/data/original/file/../game/version.txt", &[AUTH], vec![], "10.0.0.2").await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::GET, "/v1/data/nope/manifest", &[AUTH], vec![], "10.0.0.2").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mod_sets_overlay_the_original() {
    let root = tmp("mods");
    game(&root);
    let m = root.join("mods/Turbo Pack/GameData/CarModel");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::write(m.join("911.crp"), b"modded").unwrap();
    std::fs::create_dir_all(root.join("mods/Turbo Pack/SaveData")).unwrap();
    std::fs::write(root.join("mods/Turbo Pack/SaveData/0.sav"), b"profile").unwrap();
    std::fs::create_dir_all(root.join("mods/empty")).unwrap();
    let st = new_state(cfg(&root, ""));
    data::rebuild_now(&st);
    let app = router(st);
    let (s, _, b) = call(&app, Method::GET, "/v1/data/sets", &[], vec![], "10.0.0.2").await;
    assert_eq!(s, StatusCode::OK);
    let t = String::from_utf8(b).unwrap();
    assert!(t.contains("mod-Turbo Pack") && t.contains("original") && !t.contains("mod-empty"), "{t}");
    let (_, _, b) = call(&app, Method::GET, "/v1/data/mod-Turbo%20Pack/file/GameData/CarModel/911.crp", &[], vec![], "10.0.0.2").await;
    assert_eq!(b, b"modded");
    let (_, _, b) = call(&app, Method::GET, "/v1/data/mod-Turbo%20Pack/file/FEData/Art/menu.fsh", &[], vec![], "10.0.0.2").await;
    assert_eq!(b, b"menu");
    let (_, _, b) = call(&app, Method::GET, "/v1/data/original/file/GameData/CarModel/911.crp", &[], vec![], "10.0.0.2").await;
    assert_eq!(b, b"0123456789", "the original set is untouched");
}

#[tokio::test(flavor = "multi_thread")]
async fn saves_conflicts_versions_and_restore() {
    let root = tmp("saves");
    let st = new_state(cfg(&root, ""));
    let app = router(st);
    let url = "/v1/saves/Danny/files/SaveData/0.sav";
    let (s, h, _) = call(&app, Method::PUT, url, &[("x-ou-device", "mac")], b"v1".to_vec(), "10.0.0.2").await;
    assert_eq!(s, StatusCode::CREATED);
    let e1 = h["etag"].to_str().unwrap().to_string();
    // the Apple TV saves on top of v1 correctly
    let (s, h, _) = call(&app, Method::PUT, url, &[("x-ou-device", "tv"), ("if-match", &e1)], b"v2".to_vec(), "10.0.0.3").await;
    assert_eq!(s, StatusCode::OK);
    let e2 = h["etag"].to_str().unwrap().to_string();
    // the Mac, still on v1, conflicts: nothing is overwritten
    let (s, h, _) = call(&app, Method::PUT, url, &[("x-ou-device", "mac"), ("if-match", &e1)], b"v2-mac".to_vec(), "10.0.0.2").await;
    assert_eq!(s, StatusCode::PRECONDITION_FAILED);
    assert_eq!(h["etag"].to_str().unwrap(), e2);
    // no If-Match on an existing file is a conflict too
    assert_eq!(call(&app, Method::PUT, url, &[], b"x".to_vec(), "10.0.0.2").await.0, StatusCode::PRECONDITION_FAILED);
    let (s, h, b) = call(&app, Method::GET, url, &[], vec![], "10.0.0.2").await;
    assert_eq!((s, b.as_slice()), (StatusCode::OK, &b"v2"[..]));
    assert_eq!(h["x-ou-writer"], "tv");
    // versions: v1 kept
    let (_, _, b) = call(&app, Method::GET, "/v1/saves/Danny/versions/SaveData/0.sav", &[], vec![], "10.0.0.2").await;
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    let versions = v["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 1);
    let id = versions[0]["id"].as_str().unwrap().to_string();
    let (_, _, b) = call(&app, Method::GET, &format!("/v1/saves/Danny/version/{id}/SaveData/0.sav"), &[], vec![], "10.0.0.2").await;
    assert_eq!(b, b"v1");
    // restore v1: current becomes v1, v2 becomes a version
    let (s, _, _) = call(&app, Method::POST, &format!("/v1/saves/Danny/restore/{id}/SaveData/0.sav"), &[], vec![], "10.0.0.2").await;
    assert_eq!(s, StatusCode::OK);
    let (_, _, b) = call(&app, Method::GET, url, &[], vec![], "10.0.0.2").await;
    assert_eq!(b, b"v1");
    let (_, _, b) = call(&app, Method::GET, "/v1/saves/Danny/versions/SaveData/0.sav", &[], vec![], "10.0.0.2").await;
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["versions"].as_array().unwrap().len(), 2);
    // only save files; no path escapes; profiles listed
    assert_eq!(call(&app, Method::PUT, "/v1/saves/Danny/files/SaveData/replay.rpl", &[], b"r".to_vec(), "10.0.0.2").await.0, StatusCode::BAD_REQUEST);
    assert_ne!(call(&app, Method::PUT, "/v1/saves/Danny/files/../../x.sav", &[], b"r".to_vec(), "10.0.0.2").await.0, StatusCode::CREATED);
    assert_eq!(call(&app, Method::PUT, "/v1/saves/..bad/files/career.json", &[], b"r".to_vec(), "10.0.0.2").await.0, StatusCode::BAD_REQUEST);
    let (_, _, b) = call(&app, Method::GET, "/v1/saves", &[], vec![], "10.0.0.2").await;
    assert!(String::from_utf8(b).unwrap().contains("Danny"));
}

#[test]
fn version_retention() {
    use saves::{FileMeta, Version, retain};
    let day = 86_400_000u64;
    let now = 100 * day + day / 2;
    let v = |t: u64| Version { id: t.to_string(), meta: FileMeta { sha256: "x".into(), size: 1, writer: "w".into(), time: t } };
    // 10 versions today, 1 each on 5 earlier days, 1 forty days ago
    let mut all: Vec<_> = (0..10).map(|i| v(now - i * 1000)).collect();
    all.extend((1..=5).map(|d| v(now - d * day)));
    all.push(v(now - 40 * day));
    let (kept, dropped) = retain(all, 3, now);
    assert_eq!(kept.len(), 3 + 5, "newest 3 + newest of each earlier recent day");
    assert_eq!(dropped.len(), 8);
    assert!(dropped.iter().any(|d| d.meta.time == now - 40 * day));
}

#[tokio::test]
async fn lobby_register_order_heartbeat_expire() {
    let root = tmp("lobby");
    let st = new_state(cfg(&root, ""));
    let app = router(st.clone());
    let body = serde_json::json!({ "name": "Danny's race", "track": "alps", "players": 1, "version": "3.2", "addresses": ["100.101.1.2", "192.168.50.20", "not-an-ip"] });
    let (s, _, b) = call(&app, Method::POST, "/v1/lobby/games", &[("content-type", "application/json")], serde_json::to_vec(&body).unwrap(), "192.168.50.20").await;
    assert_eq!(s, StatusCode::CREATED);
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    // LAN requester: LAN address first
    let (_, _, b) = call(&app, Method::GET, "/v1/lobby/games", &[], vec![], "192.168.50.30").await;
    let g: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(g["games"][0]["addresses"], serde_json::json!(["192.168.50.20", "100.101.1.2"]));
    // tailnet requester: tailnet address first (by peer, or by ?via=)
    let (_, _, b) = call(&app, Method::GET, "/v1/lobby/games", &[], vec![], "100.90.0.1").await;
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&b).unwrap()["games"][0]["addresses"][0], "100.101.1.2");
    let (_, _, b) = call(&app, Method::GET, "/v1/lobby/games?via=tailnet", &[], vec![], "172.17.0.1").await;
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&b).unwrap()["games"][0]["addresses"][0], "100.101.1.2");
    // heartbeat updates
    let hb = serde_json::to_vec(&serde_json::json!({ "players": 2, "started": true })).unwrap();
    assert_eq!(call(&app, Method::PUT, &format!("/v1/lobby/games/{id}"), &[("content-type", "application/json")], hb, "192.168.50.20").await.0, StatusCode::NO_CONTENT);
    let (_, _, b) = call(&app, Method::GET, "/v1/lobby/games", &[], vec![], "192.168.50.30").await;
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&b).unwrap()["games"][0]["players"], 2);
    // expiry without heartbeats
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (_, _, b) = call(&app, Method::GET, "/v1/lobby/games", &[], vec![], "192.168.50.30").await;
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&b).unwrap()["games"], serde_json::json!([]));
    assert_eq!(call(&app, Method::DELETE, &format!("/v1/lobby/games/{id}"), &[], vec![], "192.168.50.20").await.0, StatusCode::NOT_FOUND);
}

#[test]
fn range_parsing() {
    use data::parse_range;
    assert_eq!(parse_range("bytes=0-9", 10), Some(Ok((0, 9))));
    assert_eq!(parse_range("bytes=5-100", 10), Some(Ok((5, 9))));
    assert_eq!(parse_range("bytes=-4", 10), Some(Ok((6, 9))));
    assert_eq!(parse_range("bytes=10-", 10), Some(Err(())));
    assert_eq!(parse_range("bytes=0-1,4-5", 10), None);
    assert_eq!(parse_range("items=0-1", 10), None);
}
