//! **Imports** (`NFSH_INBOX_DIR`): drop a game's disc image or an official update into `inbox/`; the server
//! recognises it **by its contents**, moves it to `sources/<game>/` (kept read-only, with its SHA-256), and rebuilds
//! that game's `original` set from all its sources by the game's recipe — in `staging/`, verified, then swapped into
//! `games/<game>/original` in one step (the previous set stays until the new one is verified; the manifests then
//! rebuild by themselves). Status page: an Imports section with each job's log and a Re-import button.
//!
//! * **nfsse**: a raw image (2352-byte sectors), ISO or cue/bin → the whole CD; `NFS_DOS.EXE` must be one of the two
//!   builds ReNFS runs ([`le`]).
//! * **porsche**: the disc → the engine's own file rules (`GameData/`, `FEData/`, the top-level files it reads) →
//!   each known update applied ([`patch`]: Porsche Unleashed 3.5, unpacked without running it).
//!
//! Copy-protected or installer executables are only ever read.

pub mod disc;
pub mod le;
pub mod patch;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A file kept in `sources/<game>/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub file: String,
    /// `disc` or `update`
    pub kind: String,
    pub sha256: String,
    pub size: u64,
    pub added: u64,
    /// what it was recognised as
    pub detail: String,
    /// a cue sheet's data file, kept beside it
    #[serde(default)]
    pub data_file: Option<String>,
}

/// One import run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub game: String,
    pub set: String,
    /// queued, working, done, failed
    pub state: String,
    pub started: u64,
    pub finished: Option<u64>,
    pub sources: Vec<String>,
    pub files: usize,
    pub bytes: u64,
    pub message: String,
}

/// A file in the inbox the server hasn't taken (yet).
#[derive(Debug, Clone, Serialize)]
pub struct InboxItem {
    pub name: String,
    pub size: u64,
    /// "waiting" (still being copied), "not recognised: …", "error: …"
    pub status: String,
}

#[derive(Default)]
struct ImportState {
    jobs: Vec<Job>,
    queue: VecDeque<String>,
    inbox: Vec<InboxItem>,
    /// path → (size, mtime, scans it stayed the same, verdict for a file left in the inbox)
    seen: HashMap<PathBuf, (u64, SystemTime, u32, Option<String>)>,
}

pub struct Imports {
    pub inbox: PathBuf,
    pub sources: PathBuf,
    pub staging: PathBuf,
    pub games: PathBuf,
    data: PathBuf,
    state: Mutex<ImportState>,
}

/// What `classify` found.
enum Found {
    Disc { game: &'static str, detail: String, files: Vec<PathBuf> },
    Update { game: &'static str, detail: String },
}

impl Imports {
    pub fn new(cfg: &crate::Config) -> Self {
        let data = cfg.data_dir.join("imports");
        let jobs = std::fs::read(data.join("jobs.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Imports { inbox: cfg.inbox_dir.clone(), sources: cfg.sources_dir.clone(), staging: cfg.staging_dir.clone(), games: cfg.games_dir.clone(), data, state: Mutex::new(ImportState { jobs, ..Default::default() }) }
    }

    pub fn enabled(&self) -> bool {
        self.inbox.is_dir()
    }

    pub fn jobs(&self) -> Vec<Job> {
        self.state.lock().unwrap().jobs.clone()
    }

    pub fn inbox_items(&self) -> Vec<InboxItem> {
        self.state.lock().unwrap().inbox.clone()
    }

    pub fn queued(&self) -> Vec<String> {
        self.state.lock().unwrap().queue.iter().cloned().collect()
    }

    /// Every game's kept sources.
    pub fn sources(&self) -> Vec<(String, Vec<Source>)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.sources) {
            let mut games: Vec<String> = rd.flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().to_str().map(str::to_string)).filter(|g| !g.starts_with('.')).collect();
            games.sort();
            for g in games {
                out.push((g.clone(), self.load_sources(&g)));
            }
        }
        out
    }

    fn load_sources(&self, game: &str) -> Vec<Source> {
        std::fs::read(self.sources.join(game).join("sources.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save_sources(&self, game: &str, list: &[Source]) -> std::io::Result<()> {
        let p = self.sources.join(game).join("sources.json");
        std::fs::write(&p, serde_json::to_vec_pretty(list).unwrap_or_default())
    }

    fn save_jobs(&self, jobs: &[Job]) {
        let _ = std::fs::create_dir_all(&self.data);
        let keep: Vec<&Job> = jobs.iter().rev().take(50).rev().collect();
        let _ = std::fs::write(self.data.join("jobs.json"), serde_json::to_vec_pretty(&keep).unwrap_or_default());
    }

    pub fn log_path(&self, id: &str) -> Option<PathBuf> {
        (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then(|| self.data.join("logs").join(format!("{id}.log")))
    }

    /// Queue a game's re-import (status page, API).
    pub fn request(&self, game: &str) -> bool {
        if !nfs_haven_manifest::game_id_ok(game) || self.load_sources(game).is_empty() {
            return false;
        }
        let mut s = self.state.lock().unwrap();
        if !s.queue.iter().any(|g| g == game) {
            s.queue.push_back(game.to_string());
        }
        true
    }

    // ---- the inbox ----

    /// One look at the inbox: files that stayed the same for two looks are taken.
    fn scan_inbox(&self) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(&self.inbox) else { return Vec::new() };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'))).collect();
        files.sort();
        // a cue's data file is taken with its cue
        let cue_data: Vec<PathBuf> = files.iter().filter(|p| ext(p) == "cue").filter_map(|c| disc::data_file(c).ok()).collect();
        let mut s = self.state.lock().unwrap();
        s.seen.retain(|p, _| files.contains(p));
        let mut ready = Vec::new();
        let mut items = Vec::new();
        for p in &files {
            let md = std::fs::metadata(p).ok();
            let (size, mtime) = (md.as_ref().map_or(0, |m| m.len()), md.and_then(|m| m.modified().ok()).unwrap_or(UNIX_EPOCH));
            let e = s.seen.entry(p.clone()).or_insert((size, mtime, 0, None));
            if e.0 != size || e.1 != mtime {
                *e = (size, mtime, 0, None);
            } else {
                e.2 += 1;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
            let status = if cue_data.contains(p) {
                "with its .cue".to_string()
            } else if let Some(v) = &e.3 {
                v.clone()
            } else if e.2 >= 1 {
                ready.push(p.clone());
                "recognising…".to_string()
            } else {
                "waiting (still being copied?)".to_string()
            };
            items.push(InboxItem { name, size, status });
        }
        s.inbox = items;
        ready
    }

    fn set_verdict(&self, p: &Path, verdict: String) {
        let mut s = self.state.lock().unwrap();
        if let Some(e) = s.seen.get_mut(p) {
            e.3 = Some(verdict.clone());
        }
        if let Some(i) = s.inbox.iter_mut().find(|i| Some(i.name.as_str()) == p.file_name().and_then(|n| n.to_str())) {
            i.status = verdict;
        }
    }

    /// What a file is, by its contents.
    fn classify(&self, p: &Path) -> Result<Option<Found>, String> {
        let e = ext(p);
        let looks_like_disc = e == "cue" || {
            let mut head = vec![0u8; 16 * 2352 + 32];
            let n = std::fs::File::open(p).and_then(|mut f| std::io::Read::read(&mut f, &mut head)).unwrap_or(0);
            head.truncate(n);
            head.starts_with(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]) || head.get(16 * 2048 + 1..16 * 2048 + 6) == Some(b"CD001")
        };
        if looks_like_disc {
            let mut d = disc::Disc::open(p)?;
            let files = d.files()?;
            let has = |name: &str| files.iter().any(|f| f.path.eq_ignore_ascii_case(name));
            let has_dir = |name: &str| files.iter().any(|f| f.path.len() > name.len() && f.path[..name.len()].eq_ignore_ascii_case(name) && f.path.as_bytes()[name.len()] == b'/');
            let mut moved = vec![p.to_path_buf()];
            if e == "cue" {
                moved.push(disc::data_file(p)?);
            }
            let size: u64 = files.iter().map(|f| f.size).sum();
            if has("NFS_DOS.EXE") {
                return Ok(Some(Found::Disc { game: "nfsse", detail: format!("The Need for Speed SE CD \"{}\", {} files, {:.0} MB", d.volume, files.len(), size as f64 / 1e6), files: moved }));
            }
            if has_dir("GameData") && has_dir("FEData") {
                return Ok(Some(Found::Disc { game: "porsche", detail: format!("Porsche Unleashed disc \"{}\", {} files, {:.0} MB", d.volume, files.len(), size as f64 / 1e6), files: moved }));
            }
            return Ok(None);
        }
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if size <= 64 << 20 {
            let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
            if let Some((patch, _)) = patch::recognise(&bytes) {
                return Ok(Some(Found::Update { game: patch.game, detail: patch.name.to_string() }));
            }
        }
        Ok(None)
    }

    /// Move a recognised file (and a cue's data file) to `sources/<game>/`, read-only, with its checksum.
    fn take(&self, p: &Path, found: Found) -> Result<String, String> {
        let (game, kind, detail, files) = match found {
            Found::Disc { game, detail, files } => (game, "disc", detail, files),
            Found::Update { game, detail } => (game, "update", detail, vec![p.to_path_buf()]),
        };
        let dir = self.sources.join(game);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut list = self.load_sources(game);
        // the checksum kept is the data's: the image itself, or a cue's data file
        let data = files.last().cloned().unwrap_or_else(|| p.to_path_buf());
        let sha = sha256_file(&data).map_err(|e| format!("{}: {e}", data.display()))?;
        let names: Vec<String> = files.iter().filter_map(|f| f.file_name().and_then(|n| n.to_str()).map(str::to_string)).collect();
        if list.iter().any(|s| s.sha256 == sha) {
            // the same file again: nothing new to keep
            for f in &files {
                let _ = std::fs::remove_file(f);
            }
            return Ok(format!("{game}: {detail} — already kept, the copy in the inbox removed"));
        }
        // a free name for each (a cue keeps naming its data file, so both move under their own names or not at all)
        if names.iter().any(|n| dir.join(n).exists()) {
            return Err(format!("sources/{game}/ already has a different file named {}", names.join(" / ")));
        }
        for f in &files {
            let dest = dir.join(f.file_name().unwrap());
            move_file(f, &dest)?;
            let _ = set_read_only(&dest);
        }
        let size = std::fs::metadata(dir.join(names.last().unwrap())).map(|m| m.len()).unwrap_or(0);
        list.push(Source { file: names[0].clone(), kind: kind.into(), sha256: sha, size, added: now(), detail: detail.clone(), data_file: (names.len() == 2).then(|| names[1].clone()) });
        self.save_sources(game, &list).map_err(|e| e.to_string())?;
        Ok(format!("{game}: {detail} ({})", names.join(", ")))
    }

    // ---- recipes ----

    fn run_job(&self, game: &str, set: &str, log: &mut Log) -> Result<(usize, u64, Vec<String>), String> {
        let sources = self.load_sources(game);
        let dir = self.sources.join(game);
        let discs: Vec<&Source> = sources.iter().filter(|s| s.kind == "disc").collect();
        let Some(disc_src) = discs.last() else { return Err(format!("no disc of {game} among its sources yet")) };
        if discs.len() > 1 {
            log.line(&format!("{} discs kept; the newest is used: {}", discs.len(), disc_src.file));
        }
        let mut used = vec![disc_src.file.clone()];
        log.line(&format!("disc: {} ({}), SHA-256 {}", disc_src.file, disc_src.detail, disc_src.sha256));
        // the kept source must still be the file recorded
        let disc_path = dir.join(&disc_src.file);
        let data_path = disc_src.data_file.as_ref().map(|f| dir.join(f)).unwrap_or_else(|| disc_path.clone());
        let sha = sha256_file(&data_path).map_err(|e| format!("{}: {e}", data_path.display()))?;
        if sha != disc_src.sha256 {
            return Err(format!("{} changed since it was imported (SHA-256 {sha})", disc_src.file));
        }
        let mut d = disc::Disc::open(&disc_path)?;
        let entries = d.files()?;
        log.line(&format!("volume \"{}\" ({}), {} files", d.volume, if d.joliet { "Joliet names" } else { "ISO 9660 names" }, entries.len()));
        let keep: Vec<&disc::Entry> = match game {
            "nfsse" => entries.iter().collect(),
            "porsche" => entries.iter().filter(|e| nfs_haven_manifest::is_game_file(nfs_haven_manifest::SetKind::Original, &e.path)).collect(),
            _ => return Err(format!("no recipe for {game}")),
        };
        let need: u64 = keep.iter().map(|e| e.size).sum();
        std::fs::create_dir_all(&self.staging).map_err(|e| format!("{}: {e}", self.staging.display()))?;
        let free = free_space(&self.staging);
        if need + need / 10 > free {
            return Err(format!("not enough space in staging: {:.0} MB needed, {:.0} MB free", need as f64 / 1e6, free as f64 / 1e6));
        }
        let stage = self.staging.join(format!("{game}-{set}-{}", log.id));
        let _ = std::fs::remove_dir_all(&stage);
        std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
        let result = (|| -> Result<(usize, u64), String> {
            for (n, e) in keep.iter().enumerate() {
                let dest = stage.join(&e.path);
                if let Some(p) = dest.parent() {
                    std::fs::create_dir_all(p).map_err(|er| er.to_string())?;
                }
                let mut f = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&dest).map_err(|er| format!("{}: {er}", dest.display()))?);
                d.copy(e, &mut f)?;
                f.flush().map_err(|er| er.to_string())?;
                if (n + 1) % 500 == 0 {
                    log.line(&format!("  {} of {} files", n + 1, keep.len()));
                }
            }
            log.line(&format!("copied {} files, {:.1} MB", keep.len(), need as f64 / 1e6));
            match game {
                "nfsse" => {
                    let exe = keep.iter().find(|e| e.path.eq_ignore_ascii_case("NFS_DOS.EXE")).ok_or("no NFS_DOS.EXE")?;
                    let bytes = std::fs::read(stage.join(&exe.path)).map_err(|e| e.to_string())?;
                    let (ok, sum) = le::renfs_accepts(&bytes).map_err(|e| format!("NFS_DOS.EXE: {e}"))?;
                    if !ok {
                        return Err(format!("NFS_DOS.EXE is not one of the builds ReNFS runs (image sum {sum:#010x}; it needs {:#010x} or {:#010x})", le::IMAGE_SUMS[0], le::IMAGE_SUMS[1]));
                    }
                    log.line(&format!("NFS_DOS.EXE: {} bytes, image sum {sum:#010x} — a build ReNFS runs", bytes.len()));
                    for top in ["FRONTEND", "GAMEDATA", "SIMDATA"] {
                        if !keep.iter().any(|e| e.path.len() > top.len() && e.path[..top.len()].eq_ignore_ascii_case(top) && e.path.as_bytes()[top.len()] == b'/') {
                            return Err(format!("the CD has no {top}/"));
                        }
                    }
                }
                "porsche" => {
                    for u in sources.iter().filter(|s| s.kind == "update") {
                        let bytes = std::fs::read(dir.join(&u.file)).map_err(|e| format!("{}: {e}", u.file))?;
                        let Some((p, installer)) = patch::recognise(&bytes) else { return Err(format!("{} is no longer a known update", u.file)) };
                        let files = patch::unpack(p, &installer)?;
                        for (path, b) in &files {
                            let dest = case_path(&stage, path);
                            if let Some(par) = dest.parent() {
                                std::fs::create_dir_all(par).map_err(|e| e.to_string())?;
                            }
                            std::fs::write(&dest, b).map_err(|e| format!("{}: {e}", dest.display()))?;
                        }
                        log.line(&format!("applied {} from {}: {}", p.name, u.file, files.iter().map(|(p, _)| *p).collect::<Vec<_>>().join(", ")));
                        // verify what the update put there
                        for f in p.files {
                            let got = sha256_file(&case_path(&stage, f.path)).map_err(|e| e.to_string())?;
                            if got != f.sha256 {
                                return Err(format!("{} after {}: unexpected contents", f.path, p.name));
                            }
                        }
                        used.push(u.file.clone());
                    }
                    if !(stage.join("GameData").is_dir() && stage.join("FEData").is_dir()) {
                        return Err("no GameData/ and FEData/ after the import".into());
                    }
                    let v = std::fs::read_to_string(case_path(&stage, "version.txt")).unwrap_or_default();
                    log.line(&format!("version.txt: {}", v.trim()));
                }
                _ => {}
            }
            // the result against what was meant to be copied
            let (mut files, mut bytes) = (0usize, 0u64);
            for e in &keep {
                let m = std::fs::metadata(stage.join(&e.path)).map_err(|er| format!("{}: {er}", e.path))?;
                if m.len() != e.size && !(game == "porsche" && sources.iter().any(|s| s.kind == "update")) {
                    return Err(format!("{}: {} bytes, the disc says {}", e.path, m.len(), e.size));
                }
                files += 1;
                bytes += m.len();
            }
            Ok((files, bytes))
        })();
        let (files, bytes) = match result {
            Ok(r) => r,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&stage);
                return Err(e);
            }
        };
        // where it came from, beside the files (hidden: no set ever lists it)
        let prov = serde_json::json!({ "game": game, "set": set, "job": log.id, "at": now(), "sources": sources.iter().filter(|s| used.contains(&s.file)).collect::<Vec<_>>() });
        let _ = std::fs::write(stage.join(".nfsh-import.json"), serde_json::to_vec_pretty(&prov).unwrap_or_default());
        let target = self.games.join(game).join(set);
        swap_into(&stage, &target).map_err(|e| {
            let _ = std::fs::remove_dir_all(&stage);
            e
        })?;
        log.line(&format!("published as {}/{} ({} files, {:.1} MB)", game, set, files, bytes as f64 / 1e6));
        Ok((files, bytes, used))
    }
}

/// A job's log: kept in `data/imports/logs/<id>.log` and on the console.
pub struct Log {
    pub id: String,
    file: Option<std::fs::File>,
}

impl Log {
    fn line(&mut self, s: &str) {
        tracing::info!(job = %self.id, "{s}");
        if let Some(f) = self.file.as_mut() {
            let _ = writeln!(f, "{s}");
        }
    }
}

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// `rel` under `root`, reusing the case of folders that already exist there (a disc's `FEDATA` vs an update's
/// `FEData`).
fn case_path(root: &Path, rel: &str) -> PathBuf {
    let mut at = root.to_path_buf();
    let parts: Vec<&str> = rel.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        let existing = std::fs::read_dir(&at).ok().and_then(|rd| rd.flatten().map(|e| e.file_name()).find(|n| n.to_str().is_some_and(|n| n.eq_ignore_ascii_case(part))));
        at = at.join(match (existing, i + 1 == parts.len()) {
            (Some(n), _) => n.to_string_lossy().into_owned(),
            (None, _) => part.to_string(),
        });
    }
    at
}

pub fn sha256_file(p: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(p)?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = std::io::Read::read(&mut f, &mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // another file system (the inbox and the sources are separate mounts): copy, check, then remove
    std::fs::copy(from, to).map_err(|e| format!("{} → {}: {e}", from.display(), to.display()))?;
    if std::fs::metadata(from).map(|m| m.len()).ok() != std::fs::metadata(to).map(|m| m.len()).ok() {
        let _ = std::fs::remove_file(to);
        return Err(format!("{}: the copy is incomplete", to.display()));
    }
    std::fs::remove_file(from).map_err(|e| e.to_string())
}

fn set_read_only(p: &Path) -> std::io::Result<()> {
    let mut perm = std::fs::metadata(p)?.permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(p, perm)
}

pub fn free_space(p: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else { return u64::MAX };
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: a NUL-terminated path and a zeroed out-struct
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return u64::MAX;
    }
    s.f_bavail as u64 * s.f_frsize as u64
}

/// Exchange two paths in one step where the system can (Linux `renameat2(RENAME_EXCHANGE)`, macOS
/// `renamex_np(RENAME_SWAP)`).
fn exchange(a: &Path, b: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let (Ok(ca), Ok(cb)) = (std::ffi::CString::new(a.as_os_str().as_bytes()), std::ffi::CString::new(b.as_os_str().as_bytes())) else { return false };
    #[cfg(target_os = "linux")]
    // SAFETY: two NUL-terminated paths
    unsafe {
        libc::renameat2(libc::AT_FDCWD, ca.as_ptr(), libc::AT_FDCWD, cb.as_ptr(), libc::RENAME_EXCHANGE) == 0
    }
    #[cfg(target_os = "macos")]
    // SAFETY: two NUL-terminated paths
    unsafe {
        libc::renamex_np(ca.as_ptr(), cb.as_ptr(), libc::RENAME_SWAP) == 0
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (ca, cb);
        false
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let (s, d) = (e.path(), to.join(e.file_name()));
        if e.file_type()?.is_dir() { copy_tree(&s, &d)? } else { std::fs::copy(&s, &d).map(|_| ())? }
    }
    Ok(())
}

/// Put the staged set at `target` in one step: onto the games folder's file system first (a rename, or a copy when
/// staging is elsewhere), then exchanged with the old set (or renamed in when there is none); the old one is removed
/// only after that.
pub fn swap_into(stage: &Path, target: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or("no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let name = target.file_name().and_then(|n| n.to_str()).ok_or("bad set name")?;
    let incoming = parent.join(format!(".{name}.incoming"));
    let _ = std::fs::remove_dir_all(&incoming);
    if std::fs::rename(stage, &incoming).is_err() {
        copy_tree(stage, &incoming).map_err(|e| format!("copy to {}: {e}", incoming.display()))?;
        let _ = std::fs::remove_dir_all(stage);
    }
    if !target.exists() {
        return std::fs::rename(&incoming, target).map_err(|e| e.to_string());
    }
    if exchange(&incoming, target) {
        // `incoming` now holds the previous set
        let _ = std::fs::remove_dir_all(&incoming);
        return Ok(());
    }
    // no exchange on this system: two renames, the old set put back if the second fails
    let previous = parent.join(format!(".{name}.previous"));
    let _ = std::fs::remove_dir_all(&previous);
    std::fs::rename(target, &previous).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&incoming, target) {
        let _ = std::fs::rename(&previous, target);
        return Err(e.to_string());
    }
    let _ = std::fs::remove_dir_all(&previous);
    Ok(())
}

/// The importer: looks at the inbox every few seconds, takes what it recognises, and runs queued jobs one at a time
/// (blocking work, on its own thread).
pub fn spawn(st: crate::Shared) {
    if !st.imports.enabled() {
        tracing::info!(inbox = %st.imports.inbox.display(), "imports off (no inbox folder)");
        return;
    }
    tracing::info!(inbox = %st.imports.inbox.display(), sources = %st.imports.sources.display(), staging = %st.imports.staging.display(), "imports on");
    let _ = std::thread::Builder::new().name("imports".into()).spawn(move || {
        let im = &st.imports;
        loop {
            for p in im.scan_inbox() {
                match im.classify(&p) {
                    Ok(Some(found)) => {
                        let game = match &found {
                            Found::Disc { game, .. } | Found::Update { game, .. } => *game,
                        };
                        match im.take(&p, found) {
                            Ok(msg) => {
                                tracing::info!("import: took {msg}");
                                im.request(game);
                            }
                            Err(e) => im.set_verdict(&p, format!("error: {e}")),
                        }
                    }
                    Ok(None) => im.set_verdict(&p, "not recognised".into()),
                    Err(e) => im.set_verdict(&p, format!("not recognised ({e})")),
                }
            }
            let next = im.state.lock().unwrap().queue.pop_front();
            if let Some(game) = next {
                let id = format!("{}-{game}", now());
                let mut job = Job { id: id.clone(), game: game.clone(), set: "original".into(), state: "working".into(), started: now(), finished: None, sources: Vec::new(), files: 0, bytes: 0, message: String::new() };
                {
                    let mut s = im.state.lock().unwrap();
                    s.jobs.push(job.clone());
                }
                let _ = std::fs::create_dir_all(im.data.join("logs"));
                let mut log = Log { id: id.clone(), file: std::fs::File::create(im.data.join("logs").join(format!("{id}.log"))).ok() };
                log.line(&format!("import of {game}/original"));
                match im.run_job(&game, "original", &mut log) {
                    Ok((files, bytes, used)) => {
                        job.state = "done".into();
                        job.files = files;
                        job.bytes = bytes;
                        job.sources = used;
                        job.message = format!("{files} files, {:.1} MB", bytes as f64 / 1e6);
                        // the manifests now
                        st.data.kick.notify_one();
                    }
                    Err(e) => {
                        log.line(&format!("failed: {e}"));
                        job.state = "failed".into();
                        job.message = e;
                    }
                }
                job.finished = Some(now());
                let mut s = im.state.lock().unwrap();
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    *j = job;
                }
                let jobs = s.jobs.clone();
                drop(s);
                im.save_jobs(&jobs);
                continue;
            }
            std::thread::sleep(Duration::from_secs(st.cfg.inbox_poll.as_secs().max(1)));
        }
    });
}

// ---- API ----

use axum::{
    Router,
    extract::{Path as UrlPath, RawQuery, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};

pub fn routes() -> Router<crate::Shared> {
    Router::new()
        .route("/v1/imports", get(list))
        .route("/v1/imports/{game}/run", post(run))
        .route("/v1/imports/logs/{id}", get(log_text))
}

async fn list(State(st): State<crate::Shared>) -> Response {
    let im = &st.imports;
    axum::Json(serde_json::json!({
        "enabled": im.enabled(),
        "inbox": im.inbox_items(),
        "queued": im.queued(),
        "sources": im.sources().into_iter().map(|(g, s)| serde_json::json!({ "game": g, "sources": s })).collect::<Vec<_>>(),
        "jobs": im.jobs(),
    }))
    .into_response()
}

/// Re-import a game from its kept sources. From the status page (a form with `?token=`) it goes back to the page.
async fn run(State(st): State<crate::Shared>, UrlPath(game): UrlPath<String>, RawQuery(q): RawQuery) -> Response {
    if !st.imports.request(&game) {
        return crate::error(StatusCode::NOT_FOUND, format!("no sources kept for {game:?}"));
    }
    match q.as_deref().and_then(|q| q.split('&').find(|kv| kv.starts_with("token="))) {
        Some(t) => (StatusCode::SEE_OTHER, [(header::LOCATION, format!("/?{t}"))]).into_response(),
        None => axum::Json(serde_json::json!({ "queued": game })).into_response(),
    }
}

async fn log_text(State(st): State<crate::Shared>, UrlPath(id): UrlPath<String>) -> Response {
    match st.imports.log_path(&id).and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(t) => ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], t).into_response(),
        None => crate::error(StatusCode::NOT_FOUND, "no such log"),
    }
}

#[cfg(test)]
mod tests;
