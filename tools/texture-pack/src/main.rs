//! `texture-pack scan <game folder> [--out DIR]` — Stage 0 of `hd-texture-plan.md`: an inventory of every texture
//! entry in the original game data (standalone `.fsh`, FSH packs inside `.viv` archives, `.crp` car models and the
//! front end's models), and the projected size of an HD pack. Reads the game files only; writes `inventory.csv`
//! and `summary.md` (default `target/texture-pack/`, git-ignored). Never writes or uploads any game file.
//!
//! Per entry: the **content hash** (SHA-256 of the entry's raw bytes, the key the engine's HD store will use),
//! source, name, size, pixel format, mips, alpha type (none / binary / colour-key / full / paint-label) and a
//! category guess. GPL-3.0-or-later.

use anyhow::{Context, Result, bail};
use nfs_formats::{bigf::Bigf, fsh::Fsh, refpack};
use sha2::Digest;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct Row {
    hash: String,
    source: String,
    entry: String,
    width: u32,
    height: u32,
    format: String,
    mips: u32,
    alpha: &'static str,
    category: &'static str,
}

fn category(source: &str, entry: &str, alpha: &str) -> &'static str {
    let s = source.to_ascii_lowercase();
    let e = entry.to_ascii_lowercase();
    if s.contains("gamedata/carmodel/") || s.ends_with(".crp") {
        if alpha == "paint-label" {
            "car-paint"
        } else if e.contains("glas") || e.contains("wind") {
            "car-glass"
        } else {
            "car-page"
        }
    } else if s.contains("gamedata/track/") {
        if s.contains("sky") || e.contains("sky") { "sky" } else { "track" }
    } else if s.contains("gamedata/render/") {
        "particle/effects"
    } else if s.contains("gamedata/dashhud/") || s.contains("hud") {
        "hud"
    } else if s.contains("fedata/models/") {
        "room-model"
    } else if s.contains("fedata/") {
        "fe-art"
    } else {
        "other"
    }
}

/// Alpha type of a decoded RGBA image. A car entry with many distinct alpha values that aren't a smooth ramp is
/// taken as the paint-slot label map (the alpha byte is a label there, `car.rs::tint_texel`).
fn alpha_type(rgba: &[u8], in_car: bool) -> &'static str {
    let mut seen = [false; 256];
    for px in rgba.chunks_exact(4) {
        seen[px[3] as usize] = true;
    }
    let distinct = seen.iter().filter(|&&b| b).count();
    let only = |vals: &[usize]| seen.iter().enumerate().all(|(i, &b)| !b || vals.contains(&i));
    if only(&[255]) {
        "none"
    } else if only(&[0, 255]) {
        // fully transparent texels whose colour is 0 = the colour-key convention
        let key = rgba.chunks_exact(4).filter(|p| p[3] == 0).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0);
        if key { "colour-key" } else { "binary" }
    } else if in_car && distinct >= 3 && distinct <= 24 {
        "paint-label"
    } else {
        "full"
    }
}

fn format_name(code: u8) -> String {
    match code & 0x7F {
        0x7B => "indexed8".into(),
        0x7D => "argb8888".into(),
        0x78 => "rgb565".into(),
        0x7E => "argb1555".into(),
        0x6D => "argb4444".into(),
        0x7F => "bgr888".into(),
        c => format!("0x{c:02X}"),
    }
}

/// Every FSH pack inside `data` (found by its `SHPI` magic), parsed.
fn fsh_packs(data: &[u8]) -> Vec<(usize, Fsh)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = data[i..].windows(4).position(|w| w == b"SHPI") {
        let off = i + p;
        if let Ok(f) = Fsh::parse(&data[off..])
            && !f.entries.is_empty()
            && (f.declared_size as usize) <= data.len() - off
        {
            let len = (f.declared_size as usize).max(4);
            out.push((off, f));
            i = off + len;
        } else {
            i = off + 4;
        }
        if i >= data.len() {
            break;
        }
    }
    out
}

fn scan_bytes(source: &str, data: &[u8], rows: &mut Vec<Row>) {
    let in_car = source.to_ascii_lowercase().contains("carmodel");
    for (off, f) in fsh_packs(data) {
        let pack = &data[off..off + (f.declared_size as usize).min(data.len() - off)];
        let mut starts: Vec<usize> = f.entries.iter().map(|e| e.offset as usize).collect();
        starts.push(pack.len());
        for (i, e) in f.entries.iter().enumerate() {
            let Some(b0) = e.blocks.first() else { continue };
            let end = starts[i + 1..].iter().copied().filter(|&s| s > e.offset as usize).min().unwrap_or(pack.len()).min(pack.len());
            let raw = pack.get(e.offset as usize..end).unwrap_or(&[]);
            let hash: String = sha2::Sha256::digest(raw).iter().take(16).map(|b| format!("{b:02x}")).collect();
            let alpha = f.decode(pack, i).map(|img| alpha_type(&img.rgba, in_car)).unwrap_or("undecodable");
            let name = e.name.trim_matches(char::from(0)).to_string();
            rows.push(Row {
                hash,
                source: source.to_string(),
                category: category(source, &name, alpha),
                entry: name,
                width: b0.width as u32,
                height: b0.height as u32,
                format: format_name(b0.code),
                mips: b0.mip_levels(),
                alpha,
            });
        }
    }
}

fn scan_file(root: &Path, rel: &str, rows: &mut Vec<Row>) -> Result<()> {
    let data = std::fs::read(root.join(rel)).with_context(|| rel.to_string())?;
    let lower = rel.to_ascii_lowercase();
    if lower.ends_with(".viv") {
        if let Ok(b) = Bigf::parse(&data) {
            for e in &b.entries {
                let d = refpack::maybe_decompress(b.data(&data, e)).map(|c| c.into_owned()).unwrap_or_else(|_| b.data(&data, e).to_vec());
                scan_bytes(&format!("{rel}/{}", e.name), &d, rows);
            }
        }
        return Ok(());
    }
    let d = refpack::maybe_decompress(&data).map(|c| c.into_owned()).unwrap_or(data);
    scan_bytes(rel, &d, rows);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("scan") || args.len() < 2 {
        bail!("usage: texture-pack scan <game folder> [--out DIR]   (export / import / fake: later stages)");
    }
    let root = PathBuf::from(&args[1]);
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/texture-pack"));
    let files = ou_manifest::list_files(&root, ou_manifest::SetKind::Original).context("game folder")?;
    let mut rows = Vec::new();
    for rel in &files {
        let l = rel.to_ascii_lowercase();
        if [".fsh", ".viv", ".crp", ".qfs"].iter().any(|x| l.ends_with(x)) {
            if let Err(e) = scan_file(&root, rel, &mut rows) {
                eprintln!("{rel}: {e}");
            }
        }
    }
    std::fs::create_dir_all(&out)?;
    let mut csv = String::from("hash,source,entry,width,height,format,mips,alpha,category\n");
    for r in &rows {
        let _ = writeln!(csv, "{},{},{},{},{},{},{},{},{}", r.hash, r.source.replace(',', ";"), r.entry.replace(',', ";"), r.width, r.height, r.format, r.mips, r.alpha, r.category);
    }
    std::fs::write(out.join("inventory.csv"), csv)?;

    // summary: unique entries (by hash) per category, texels, projected pack sizes
    let mut uniq: BTreeMap<String, &Row> = BTreeMap::new();
    for r in &rows {
        uniq.entry(r.hash.clone()).or_insert(r);
    }
    #[derive(Default)]
    struct Cat {
        n: usize,
        texels: u64,
    }
    let mut cats: BTreeMap<&str, Cat> = BTreeMap::new();
    for r in uniq.values() {
        let c = cats.entry(r.category).or_default();
        c.n += 1;
        c.texels += r.width as u64 * r.height as u64;
    }
    // ASTC bits per texel: 4×4 = 8, 6×6 ≈ 3.56; world textures (track, sky, room models) 6×6, the rest 4×4; mips ×4/3
    let bpp = |cat: &str| if matches!(cat, "track" | "sky" | "room-model") { 128.0 / 36.0 } else { 8.0 };
    let size = |scale: u64| -> f64 { cats.iter().map(|(k, c)| (c.texels * scale * scale) as f64 * bpp(k) / 8.0 * 4.0 / 3.0).sum() };
    let mut md = String::new();
    let _ = writeln!(md, "# HD texture pack — Stage 0 inventory\n\nGame folder: `{}`\n", root.display());
    let _ = writeln!(md, "Entries found: {} ({} unique by content hash) in {} files scanned.\n", rows.len(), uniq.len(), files.iter().filter(|f| [".fsh", ".viv", ".crp", ".qfs"].iter().any(|x| f.to_ascii_lowercase().ends_with(x))).count());
    let _ = writeln!(md, "| Category | Unique entries | Texels (1×) | 2× pack (ASTC) | 4× pack (ASTC) |\n|---|---:|---:|---:|---:|");
    for (k, c) in &cats {
        let mb = |s: u64| (c.texels * s * s) as f64 * bpp(k) / 8.0 * 4.0 / 3.0 / 1e6;
        let _ = writeln!(md, "| {k} | {} | {} | {:.1} MB | {:.1} MB |", c.n, c.texels, mb(2), mb(4));
    }
    let _ = writeln!(md, "| **total** | {} | {} | **{:.0} MB** | **{:.0} MB** |", uniq.len(), cats.values().map(|c| c.texels).sum::<u64>(), size(2) / 1e6, size(4) / 1e6);
    let mut alphas: BTreeMap<&str, usize> = BTreeMap::new();
    for r in uniq.values() {
        *alphas.entry(r.alpha).or_default() += 1;
    }
    let _ = writeln!(md, "\nAlpha types (unique entries): {}", alphas.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
    let _ = writeln!(md, "\nBudget (plan): HD pack ≤ 1.5 GB. ASTC 6×6 for track / sky / room models, 4×4 for the rest, mips included.");
    std::fs::write(out.join("summary.md"), &md)?;
    print!("{md}");
    eprintln!("wrote {} and {}", out.join("inventory.csv").display(), out.join("summary.md").display());
    Ok(())
}
