//! `texture-pack scan <game folder> [--out DIR]` — Stage 0 of `hd-texture-plan.md`: an inventory of every texture
//! entry in the original game data (standalone `.fsh`, FSH packs inside `.viv` archives, `.crp` car models and the
//! front end's models), and the projected size of an HD pack. Reads the game files only; writes `inventory.csv`
//! and `summary.md` (default `target/texture-pack/`, git-ignored). Never writes or uploads any game file.
//!
//! `texture-pack fake <game folder> --out DIR [--scale 2] [--only cat,cat] [--match text] [--astc]` — Stage 1's
//! **stand-in pack**: every matching entry upscaled nearest-neighbour (no AI), written as the engine's HD store reads
//! it (`index.json` + one KTX2 per entry, RGBA8, or ASTC 4×4 with `--astc` when ARM's `astcenc` is on the PATH or
//! in `ASTCENC`). The game must look the same with it, only with N× the pixels: it proves the hooks. Car
//! categories are never included (the engine keeps cars original for now). Personal use only, never shared.
//!
//! Per entry: the **content hash** (SHA-256 of the entry's raw bytes, `nfs_assets::hd::entry_key` — the key the
//! engine's HD store files it under),
//! source, name, size, pixel format, mips, alpha type (none / binary / colour-key / full / paint-label) and a
//! category guess. GPL-3.0-or-later.

use anyhow::{Context, Result, bail};
use nfs_formats::{bigf::Bigf, fsh::Fsh, refpack};
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
        for (i, e) in f.entries.iter().enumerate() {
            let Some(b0) = e.blocks.first() else { continue };
            let hash = nfs_assets::hd::entry_key(f.entry_bytes(pack, i));
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
    if args.first().map(String::as_str) == Some("fake") && args.len() >= 2 {
        return fake(&args);
    }
    if args.first().map(String::as_str) != Some("scan") || args.len() < 2 {
        bail!("usage: texture-pack scan <game folder> [--out DIR]\n       texture-pack fake <game folder> --out DIR [--scale 2] [--only cat,cat] [--match text] [--astc]");
    }
    let root = PathBuf::from(&args[1]);
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("target/texture-pack"));
    let files = nfs_haven_manifest::list_files(&root, nfs_haven_manifest::SetKind::Original).context("game folder")?;
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
    // ASTC 4×4 = 8 bits per texel for everything: the GPU takes block-compressed textures only at sizes that are a
    // multiple of the block, so 6×6 (≈3.6 bits) doesn't fit the game's power-of-two textures
    let bpp = |_cat: &str| 8.0;
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
    let _ = writeln!(md, "\nBudget (plan): HD pack ≤ 1.5 GB. ASTC 4×4 throughout (6×6 needs sizes that are multiples of 6, which the game's power-of-two textures aren't), mips included.");
    std::fs::write(out.join("summary.md"), &md)?;
    print!("{md}");
    eprintln!("wrote {} and {}", out.join("inventory.csv").display(), out.join("summary.md").display());
    Ok(())
}

// ---- fake: the stand-in pack ----

fn nearest(rgba: &[u8], w: u32, h: u32, s: u32) -> Vec<u8> {
    let (nw, nh) = (w * s, h * s);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = (x / s, y / s);
            let si = ((sy * w + sx) * 4) as usize;
            let di = ((y * nw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
        }
    }
    out
}

/// ASTC 4×4 blocks of one level with ARM's `astcenc` (`-medium`, LDR), or `None` when it isn't there or fails.
fn astc_encode(astcenc: &Path, tmp: &Path, rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let png = tmp.join("in.png");
    let out = tmp.join("out.astc");
    image::save_buffer(&png, rgba, w, h, image::ColorType::Rgba8).ok()?;
    let ok = std::process::Command::new(astcenc).args(["-cl"]).arg(&png).arg(&out).args(["4x4", "-medium", "-silent"]).status().ok()?.success();
    if !ok {
        return None;
    }
    let d = std::fs::read(&out).ok()?;
    // the .astc header: magic, block size, 3×24-bit dimensions (16 bytes)
    d.get(16..).map(<[u8]>::to_vec)
}

fn fake(args: &[String]) -> Result<()> {
    use nfs_renderer::TexFormat;
    let root = PathBuf::from(&args[1]);
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let out = PathBuf::from(opt("--out").context("--out DIR")?);
    let scale: u32 = opt("--scale").map(|v| v.parse()).transpose()?.unwrap_or(2);
    if !matches!(scale, 2 | 4) {
        bail!("--scale 2 or 4");
    }
    let only: Option<Vec<String>> = opt("--only").map(|v| v.split(',').map(str::to_string).collect());
    let matching = opt("--match").map(|m| m.to_ascii_lowercase());
    let astcenc = args.iter().any(|a| a == "--astc").then(|| {
        std::env::var_os("ASTCENC").map(PathBuf::from).or_else(|| {
            std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("astcenc")).find(|c| c.is_file()))
        })
    });
    let astcenc = match astcenc {
        Some(Some(p)) => Some(p),
        Some(None) => bail!("--astc: astcenc not found (set ASTCENC or put it on the PATH)"),
        None => None,
    };
    let tmp = out.join(".tmp");
    std::fs::create_dir_all(&tmp)?;
    let files = nfs_haven_manifest::list_files(&root, nfs_haven_manifest::SetKind::Original).context("game folder")?;
    let mut index = serde_json::Map::new();
    let (mut n_rgba, mut n_astc, mut bytes) = (0usize, 0usize, 0u64);
    for rel in &files {
        let l = rel.to_ascii_lowercase();
        if !l.ends_with(".fsh") || matching.as_ref().is_some_and(|m| !l.contains(m.as_str())) {
            continue;
        }
        let data = std::fs::read(root.join(rel))?;
        let data = refpack::maybe_decompress(&data).map(|c| c.into_owned()).unwrap_or(data);
        let Ok(f) = Fsh::parse(&data) else { continue };
        for i in 0..f.entries.len() {
            let e = &f.entries[i];
            let Some(b0) = e.blocks.first() else { continue };
            let key = nfs_assets::hd::entry_key(f.entry_bytes(&data, i));
            if index.contains_key(&key) {
                continue;
            }
            let Ok(levels) = f.decode_levels(&data, i, b0.mip_levels()) else { continue };
            let Some(top) = levels.first() else { continue };
            let cat = category(rel, &e.name, alpha_type(&top.rgba, false));
            if cat.starts_with("car") || only.as_ref().is_some_and(|o| !o.iter().any(|c| c == cat)) {
                continue;
            }
            // the chain the engine can take: each level exactly half the one before (an odd size ends it)
            let (w, h) = (top.width * scale, top.height * scale);
            let mut chain = Vec::new();
            let (mut lw, mut lh) = (w, h);
            for lv in &levels {
                if lv.width * scale != lw || lv.height * scale != lh {
                    break;
                }
                chain.push(nearest(&lv.rgba, lv.width, lv.height, scale));
                lw = (lw / 2).max(1);
                lh = (lh / 2).max(1);
            }
            // ASTC 4×4 needs a top level that's whole blocks; anything else stays RGBA8
            let astc: Option<Vec<Vec<u8>>> = match &astcenc {
                Some(enc) if w % 4 == 0 && h % 4 == 0 => {
                    let (mut cw, mut ch) = (w, h);
                    chain
                        .iter()
                        .map(|lv| {
                            let r = astc_encode(enc, &tmp, lv, cw, ch);
                            cw = (cw / 2).max(1);
                            ch = (ch / 2).max(1);
                            r
                        })
                        .collect()
                }
                _ => None,
            };
            let file = format!("{key}.ktx2");
            let ktx = match &astc {
                Some(blocks) => {
                    n_astc += 1;
                    nfs_assets::hd::write_ktx2(w, h, TexFormat::Astc4x4, blocks)
                }
                None => {
                    n_rgba += 1;
                    nfs_assets::hd::write_ktx2(w, h, TexFormat::Rgba8, &chain)
                }
            };
            bytes += ktx.len() as u64;
            std::fs::write(out.join(&file), ktx)?;
            index.insert(key, serde_json::json!({ "file": file, "category": cat, "source": format!("{rel}/{}", e.name.trim_matches(char::from(0))) }));
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    let ix = serde_json::json!({ "format": 1, "scale": scale, "kind": "fake (nearest-neighbour)", "entries": index });
    std::fs::write(out.join("index.json"), serde_json::to_string_pretty(&ix)?)?;
    eprintln!("fake pack: {} entries ({n_astc} ASTC 4×4, {n_rgba} RGBA8), {:.1} MB, scale {scale}× → {}", n_astc + n_rgba, bytes as f64 / 1e6, out.display());
    Ok(())
}
