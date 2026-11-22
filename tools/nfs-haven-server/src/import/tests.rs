//! Imports on made-up discs and updates (no game files): the disc reader, the recipes, the swap, the inbox.

use super::*;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nfsh-import-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn importer(root: &Path) -> Imports {
    for d in ["inbox", "sources", "staging", "games", "data"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    let cfg = crate::Config {
        token: String::new(),
        port: 0,
        name: "t".into(),
        save_versions: 3,
        lobby_ttl: Duration::from_secs(5),
        games_dir: root.join("games"),
        data_dir: root.join("data"),
        inbox_dir: root.join("inbox"),
        sources_dir: root.join("sources"),
        staging_dir: root.join("staging"),
        inbox_poll: Duration::from_millis(10),
        rescan: Duration::from_secs(60),
        save_max_bytes: 1 << 20,
    };
    Imports::new(&cfg)
}

/// What the importer thread does for one file in the inbox, then the queued jobs.
fn step(im: &Imports) -> Vec<Result<String, String>> {
    im.scan_inbox();
    for p in im.scan_inbox() {
        match im.classify(&p) {
            Ok(Some(f)) => {
                let game = match &f {
                    Found::Disc { game, .. } | Found::Update { game, .. } => *game,
                };
                im.take(&p, f).unwrap();
                im.request(game);
            }
            Ok(None) => im.set_verdict(&p, "not recognised".into()),
            Err(e) => im.set_verdict(&p, format!("not recognised ({e})")),
        }
    }
    let mut out = Vec::new();
    while let Some(game) = { let g = im.state.lock().unwrap().queue.pop_front(); g } {
        let mut log = Log { id: format!("t-{game}"), file: None };
        out.push(im.run_job(&game, "original", &mut log).map(|(n, b, _)| format!("{n} files {b} bytes")));
    }
    out
}

#[test]
fn the_disc_reader_reads_raw_sectors_and_joliet_names() {
    let root = tmp("disc");
    let big: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
    let img = disc::make_test_image(&[("GameData/Track.viv", &big), ("version.txt", b"3.2"), ("FEData/a.fsh", b"fsh")], true);
    let p = root.join("disc.img");
    std::fs::write(&p, &img).unwrap();
    let mut d = disc::Disc::open(&p).unwrap();
    assert!(d.joliet);
    let files = d.files().unwrap();
    assert_eq!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["FEData/a.fsh", "GameData/Track.viv", "version.txt"]);
    assert_eq!(d.read(&files[1]).unwrap(), big);
    // the same as a plain ISO, and through a cue sheet
    std::fs::write(root.join("plain.iso"), disc::make_test_image(&[("version.txt", b"3.2")], false)).unwrap();
    assert_eq!(disc::Disc::open(&root.join("plain.iso")).unwrap().files().unwrap()[0].path, "version.txt");
    std::fs::write(root.join("d.cue"), "FILE \"disc.img\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n").unwrap();
    assert_eq!(disc::Disc::open(&root.join("d.cue")).unwrap().files().unwrap().len(), 3);
}

#[test]
fn a_porsche_disc_becomes_the_original_set_by_the_engines_rules() {
    let root = tmp("porsche");
    let im = importer(&root);
    let img = disc::make_test_image(&[("GameData/Track.viv", b"track"), ("FEData/menu.fsh", b"menu"), ("version.txt", b"3.2"), ("Porsche.exe", b"protected"), ("readme.txt", b"hi")], true);
    std::fs::write(root.join("inbox/Porsche.bin"), &img).unwrap();
    std::fs::write(root.join("inbox/Porsche.cue"), "FILE \"Porsche.bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n").unwrap();
    std::fs::write(root.join("inbox/notes.pdf"), b"something else").unwrap();
    let r = step(&im);
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(r[0].is_ok(), "{r:?}");
    let set = root.join("games/porsche/original");
    assert_eq!(std::fs::read(set.join("GameData/Track.viv")).unwrap(), b"track");
    assert_eq!(std::fs::read(set.join("version.txt")).unwrap(), b"3.2");
    assert!(!set.join("Porsche.exe").exists() && !set.join("readme.txt").exists(), "only the files the engine reads");
    assert!(set.join(".nfsh-import.json").exists());
    // the sources: moved, read-only, with the data file's checksum
    let kept = im.load_sources("porsche");
    assert_eq!((kept[0].file.as_str(), kept[0].data_file.as_deref(), kept[0].kind.as_str()), ("Porsche.cue", Some("Porsche.bin"), "disc"));
    assert_eq!(kept[0].sha256, patch::sha256_hex(&img));
    assert!(std::fs::metadata(root.join("sources/porsche/Porsche.bin")).unwrap().permissions().readonly());
    assert!(!root.join("inbox/Porsche.bin").exists());
    // the unknown file stays, marked
    assert!(root.join("inbox/notes.pdf").exists());
    assert!(im.inbox_items().iter().any(|i| i.name == "notes.pdf" && i.status.starts_with("not recognised")));
    // a re-import swaps a fresh copy in; nothing left in staging
    std::fs::write(set.join("GameData/Track.viv"), b"changed by hand").unwrap();
    assert!(im.request("porsche"));
    assert!(step(&im)[0].is_ok());
    assert_eq!(std::fs::read(set.join("GameData/Track.viv")).unwrap(), b"track");
    assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);
    assert!(std::fs::read_dir(root.join("games/porsche")).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with('.')));
}

#[test]
fn an_nfsse_cd_with_an_unknown_program_is_refused_and_the_old_set_kept() {
    let root = tmp("nfsse");
    let im = importer(&root);
    std::fs::create_dir_all(root.join("games/nfsse/original")).unwrap();
    std::fs::write(root.join("games/nfsse/original/OLD.TXT"), b"previous").unwrap();
    let img = disc::make_test_image(&[("NFS_DOS.EXE", b"MZ not really"), ("GAMEDATA/X.DAT", b"x"), ("FRONTEND/Y.DAT", b"y"), ("SIMDATA/Z.DAT", b"z")], true);
    std::fs::write(root.join("inbox/NFSSE.img"), &img).unwrap();
    let r = step(&im);
    assert!(r[0].as_ref().is_err_and(|e| e.contains("NFS_DOS.EXE")), "{r:?}");
    assert_eq!(std::fs::read(root.join("games/nfsse/original/OLD.TXT")).unwrap(), b"previous");
    assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);
    assert_eq!(im.load_sources("nfsse")[0].detail.split(',').next().unwrap(), "The Need for Speed SE CD \"TEST\"");
}

#[test]
fn an_updates_files_are_found_among_its_streams_by_checksum() {
    use std::io::Write as _;
    let deflate = |b: &[u8]| {
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(b).unwrap();
        e.finish().unwrap()
    };
    let a = b"version 3.5 of something".repeat(20);
    let b = b"3.5".to_vec();
    let mut installer = b"MZ junk junk junk".to_vec();
    installer.extend(deflate(&b"a file the table doesn't list".repeat(10)));
    installer.extend(deflate(&b));
    installer.extend(b"\xff\xfe more junk");
    installer.extend(deflate(&a));
    let files: &'static [patch::PatchFile] = Box::leak(Box::new([
        patch::PatchFile { path: "FEData/a.txt", sha256: Box::leak(patch::sha256_hex(&a).into_boxed_str()), size: a.len() as u64 },
        patch::PatchFile { path: "version.txt", sha256: Box::leak(patch::sha256_hex(&b).into_boxed_str()), size: 3 },
    ]));
    let p = patch::Patch { game: "porsche", name: "test", installer_sha256: "", download_sha256: &[], files };
    let out = patch::unpack(&p, &installer).unwrap();
    assert_eq!(out, vec![("FEData/a.txt", a.clone()), ("version.txt", b.clone())]);
    let short = &installer[..installer.len() - 5];
    assert!(patch::unpack(&p, short).unwrap_err().contains("FEData/a.txt"));
    assert!(patch::recognise(b"not an update").is_none());
    assert!(le::image_sum(b"MZ").is_err());
}
