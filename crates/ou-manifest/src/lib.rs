//! Game-data manifests: the list of files (relative path, size, SHA-256) the Apple TV app downloads.
//!
//! Shared by `tools/make-manifest` and the OpenUnleashed server (`tools/ou-server`) so both produce the same
//! manifest byte for byte for the same folder. Which files belong to a set is decided here too
//! ([`is_game_file`]), following `docs/en/game-data.md`. Nothing here reads or interprets game files beyond
//! hashing their bytes. GPL-3.0-or-later.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Manifest format version (bump on any incompatible change).
pub const FORMAT: u32 = 1;

/// One file of a data set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Relative path with `/` separators, e.g. `GameData/Track/alps/...`.
    pub path: String,
    pub size: u64,
    /// Lower-case hex SHA-256 of the file's bytes.
    pub sha256: String,
}

/// A data set's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    /// `original`, `mod:<name>` or `hd`.
    pub set: String,
    pub total_size: u64,
    /// Sorted by `path` (byte order).
    pub files: Vec<Entry>,
}

impl Manifest {
    pub fn new(set: &str, mut files: Vec<Entry>) -> Self {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files.dedup_by(|a, b| a.path == b.path);
        let total_size = files.iter().map(|f| f.size).sum();
        Self { format: FORMAT, set: set.to_string(), total_size, files }
    }

    /// The canonical JSON (pretty, `\n`-terminated) — what `make-manifest` writes and the server serves.
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("manifest serialises");
        s.push('\n');
        s
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.files.binary_search_by(|e| e.path.as_str().cmp(path)).ok().map(|i| &self.files[i])
    }
}

/// What a data set is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    /// The original game folder: `GameData/`, `FEData/`, `regs/` and the few top-level files the engine reads.
    Original,
    /// A mod pack overlaid on the original: `GameData/`, `FEData/`, `SaveData/` (mod profiles).
    Mod,
    /// The HD texture pack: every file.
    Hd,
}

/// Top-level files of the original the engine reads (`docs/en/game-data.md`; `nfs5.ini` = the Verok patch's
/// settings, `regs/*.reg` = the installer's registry export).
const ORIGINAL_TOP_FILES: [&str; 4] = ["version.txt", "nfs5.exe", "fe.txt", "nfs5.ini"];
const ORIGINAL_DIRS: [&str; 3] = ["GameData", "FEData", "regs"];
const MOD_DIRS: [&str; 3] = ["GameData", "FEData", "SaveData"];

/// Whether a relative path (with `/`) belongs to a set of `kind`. Hidden files (`.DS_Store`, `._*`) never do.
pub fn is_game_file(kind: SetKind, rel: &str) -> bool {
    if rel.split('/').any(|c| c.starts_with('.') || c.is_empty()) {
        return false;
    }
    let (first, rest) = match rel.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (rel, None),
    };
    match kind {
        SetKind::Hd => true,
        SetKind::Original => match rest {
            None => ORIGINAL_TOP_FILES.iter().any(|f| f.eq_ignore_ascii_case(first)),
            Some(_) => ORIGINAL_DIRS.iter().any(|d| d.eq_ignore_ascii_case(first)),
        },
        SetKind::Mod => rest.is_some() && MOD_DIRS.iter().any(|d| d.eq_ignore_ascii_case(first)),
    }
}

/// Every file of `root` that belongs to a set of `kind`, as sorted relative paths with `/`.
/// Symbolic links are not followed (a set never reaches outside its folder).
pub fn list_files(root: &Path, kind: SetKind) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel_dir) = stack.pop() {
        let rd = match std::fs::read_dir(root.join(&rel_dir)) {
            Ok(rd) => rd,
            Err(e) if rel_dir.as_os_str().is_empty() => return Err(e),
            Err(_) => continue,
        };
        for ent in rd {
            let ent = ent?;
            let ft = ent.file_type()?;
            let Some(name) = ent.file_name().to_str().map(str::to_string) else { continue };
            let rel = rel_dir.join(&name);
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            if ft.is_dir() {
                // only descend into directories that can hold set files
                if rel_dir.as_os_str().is_empty() && kind != SetKind::Hd {
                    let dirs: &[&str] = if kind == SetKind::Original { &ORIGINAL_DIRS } else { &MOD_DIRS };
                    if !dirs.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                        continue;
                    }
                }
                if !name.starts_with('.') {
                    stack.push(rel);
                }
            } else if ft.is_file() && is_game_file(kind, &rel_s) {
                out.push(rel_s);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// SHA-256 and size of a file, streamed.
pub fn hash_file(path: &Path) -> std::io::Result<(u64, String)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((size, to_hex(&h.finalize())))
}

/// Lower-case hex.
pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

/// SHA-256 of a byte slice, hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// Remembered hashes keyed by path + size + modification time, so a rebuild doesn't re-hash ~0.5 GB.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HashCache {
    entries: BTreeMap<String, CachedHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHash {
    size: u64,
    mtime_ns: u128,
    sha256: String,
}

impl HashCache {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).expect("cache serialises"))?;
        std::fs::rename(tmp, path)
    }

    /// The hash of `root/rel`, from the cache when size and mtime still match.
    pub fn hash(&mut self, root: &Path, rel: &str) -> std::io::Result<(u64, String)> {
        let full = root.join(rel);
        let md = std::fs::metadata(&full)?;
        let mtime_ns = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
        let key = format!("{}\0{rel}", root.display());
        if let Some(c) = self.entries.get(&key)
            && c.size == md.len()
            && c.mtime_ns == mtime_ns
        {
            return Ok((c.size, c.sha256.clone()));
        }
        let (size, sha256) = hash_file(&full)?;
        self.entries.insert(key, CachedHash { size, mtime_ns, sha256: sha256.clone() });
        Ok((size, sha256))
    }
}

/// The manifest of one folder.
pub fn build(root: &Path, kind: SetKind, set: &str, cache: &mut HashCache) -> std::io::Result<Manifest> {
    let mut files = Vec::new();
    for rel in list_files(root, kind)? {
        let (size, sha256) = cache.hash(root, &rel)?;
        files.push(Entry { path: rel, size, sha256 });
    }
    Ok(Manifest::new(set, files))
}

/// Where each file of an overlay set comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Base,
    Overlay,
}

/// A mod set: the original's files with the mod's on top. A mod file replaces the original file with the same path,
/// compared case-insensitively (the original game ran on Windows, so mods don't always match its case); the mod's
/// spelling wins. Returns the manifest and, per path, where the file comes from.
pub fn overlay(base: &Manifest, overlay: &Manifest, set: &str) -> (Manifest, BTreeMap<String, (Origin, String)>) {
    let mut by_key: BTreeMap<String, (Entry, Origin, String)> = BTreeMap::new();
    for e in &base.files {
        by_key.insert(e.path.to_ascii_lowercase(), (e.clone(), Origin::Base, e.path.clone()));
    }
    for e in &overlay.files {
        by_key.insert(e.path.to_ascii_lowercase(), (e.clone(), Origin::Overlay, e.path.clone()));
    }
    let mut origins = BTreeMap::new();
    let mut files = Vec::with_capacity(by_key.len());
    for (_, (e, o, src)) in by_key {
        origins.insert(e.path.clone(), (o, src));
        files.push(e);
    }
    (Manifest::new(set, files), origins)
}

/// A relative path that is safe to join onto a folder: no absolute path, no `..`, no empty or hidden component,
/// no backslash or NUL.
pub fn safe_rel_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains('\0')
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != ".." && !c.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ou-manifest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn original_set_membership() {
        assert!(is_game_file(SetKind::Original, "GameData/Track/a.viv"));
        assert!(is_game_file(SetKind::Original, "FEData/Art/x.fsh"));
        assert!(is_game_file(SetKind::Original, "version.txt"));
        assert!(is_game_file(SetKind::Original, "regs/nfs5.reg"));
        assert!(!is_game_file(SetKind::Original, "Porsche.exe"));
        assert!(!is_game_file(SetKind::Original, "readme.txt"));
        assert!(!is_game_file(SetKind::Original, "GameData/.DS_Store"));
        assert!(is_game_file(SetKind::Mod, "SaveData/0.sav"));
        assert!(!is_game_file(SetKind::Mod, "readme.txt"));
    }

    #[test]
    fn build_is_stable_and_cached() {
        let d = tmp("build");
        std::fs::create_dir_all(d.join("GameData/Track")).unwrap();
        std::fs::create_dir_all(d.join("FEData")).unwrap();
        std::fs::write(d.join("GameData/Track/a.bin"), b"hello").unwrap();
        std::fs::write(d.join("FEData/b.txt"), b"world").unwrap();
        std::fs::write(d.join("version.txt"), b"3.2").unwrap();
        std::fs::write(d.join("Porsche.exe"), b"no").unwrap();
        let mut c = HashCache::default();
        let m = build(&d, SetKind::Original, "original", &mut c).unwrap();
        assert_eq!(m.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["FEData/b.txt", "GameData/Track/a.bin", "version.txt"]);
        assert_eq!(m.total_size, 13);
        assert_eq!(m.get("GameData/Track/a.bin").unwrap().sha256, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        let m2 = build(&d, SetKind::Original, "original", &mut c).unwrap();
        assert_eq!(m.to_json(), m2.to_json());
    }

    #[test]
    fn overlay_replaces_case_insensitively() {
        let e = |p: &str, s: &str| Entry { path: p.into(), size: 1, sha256: s.into() };
        let base = Manifest::new("original", vec![e("GameData/CarModel/911.crp", "a"), e("FEData/x", "b")]);
        let m = Manifest::new("mod", vec![e("GameData/carmodel/911.CRP", "c"), e("SaveData/0.sav", "d")]);
        let (o, origins) = overlay(&base, &m, "mod:test");
        assert_eq!(o.files.len(), 3);
        assert_eq!(o.get("GameData/carmodel/911.CRP").unwrap().sha256, "c");
        assert_eq!(origins["FEData/x"].0, Origin::Base);
        assert_eq!(origins["SaveData/0.sav"].0, Origin::Overlay);
    }

    #[test]
    fn path_safety() {
        assert!(safe_rel_path("GameData/a.viv"));
        for p in ["", "/etc/passwd", "../x", "a/../b", "a//b", "a\\b", ".hidden", "a/.x"] {
            assert!(!safe_rel_path(p), "{p}");
        }
    }
}
