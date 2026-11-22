//! `make-manifest <folder> [-o manifest.json] [--game porsche] [--set original|mod|hd] [--name NAME]`
//!
//! Walks a game folder and writes `manifest.json`: every file of the set (relative path, size, SHA-256), the
//! same bytes nfs-haven-server serves for that folder (`crates/nfs-haven-manifest`). GPL-3.0-or-later.

use nfs_haven_manifest::{HashCache, SetKind};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut root = None;
    let mut out = None;
    let mut kind = SetKind::Original;
    let mut name = None;
    let mut game = String::from("porsche");
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--output" => out = args.next().map(PathBuf::from),
            "--set" => {
                kind = match args.next().as_deref() {
                    Some("original") => nfs_haven_manifest::kinds_for(&game).0,
                    Some("mod") => nfs_haven_manifest::kinds_for(&game).1,
                    Some("hd") => SetKind::Hd,
                    other => die(&format!("unknown set kind {other:?} (original, mod, hd)")),
                }
            }
            "--name" => name = args.next(),
            "--game" => game = args.next().unwrap_or_else(|| die("--game needs an id")),
            "-h" | "--help" => {
                println!("usage: make-manifest <folder> [-o manifest.json] [--game porsche] [--set original|mod|hd] [--name NAME]");
                return;
            }
            _ if root.is_none() => root = Some(PathBuf::from(a)),
            _ => die(&format!("unexpected argument {a}")),
        }
    }
    let root = root.unwrap_or_else(|| die("missing <game folder>"));
    if kind == SetKind::Original && !(root.join("GameData").is_dir() && root.join("FEData").is_dir()) {
        die(&format!("{} has no GameData/ and FEData/ (see docs/en/game-data.md)", root.display()));
    }
    let set = format!("{game}/{}", name.unwrap_or_else(|| match kind {
        SetKind::Original | SetKind::Any => "original".into(),
        SetKind::Mod => "mod".into(),
        SetKind::Hd => "hd".into(),
    }));
    let mut cache = HashCache::default();
    let m = nfs_haven_manifest::build(&root, kind, &set, &mut cache).unwrap_or_else(|e| die(&format!("{}: {e}", root.display())));
    let out = out.unwrap_or_else(|| PathBuf::from("manifest.json"));
    std::fs::write(&out, m.to_json()).unwrap_or_else(|e| die(&format!("{}: {e}", out.display())));
    eprintln!("{}: {} files, {:.1} MB", out.display(), m.files.len(), m.total_size as f64 / 1e6);
}

fn die(msg: &str) -> ! {
    eprintln!("make-manifest: {msg}");
    std::process::exit(2)
}
