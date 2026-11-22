//! **PORT-DEVIATION (HD texture packs, `hd-texture-plan.md` Stage 1)** — the "HD store": optional higher-resolution
//! copies of the game's own texture entries, made from the player's own copy (never shipped). Nothing changes
//! without a pack, or with Extras › Graphics › Textures on "Original".
//!
//! * **Key** = the content hash of an entry's raw bytes ([`entry_key`] of [`nfs_formats::fsh::Fsh::entry_bytes`]:
//!   image, palette and attachments, before any tint or cache conversion), so one HD texture covers the entry
//!   wherever it is loaded from. `tools/texture-pack` computes the same key.
//! * **Pack** = a folder with `index.json` (`{format: 1, scale, entries: {key: {file, …}}}`) and one KTX2 file per
//!   entry (RGBA8, or ASTC 4×4 where the GPU samples it), each with the entry's level chain at `scale`×. Found at
//!   `NFS_HD_PATH`, else `hd/` in the data folder (tvOS: `Caches/games/porsche/hd`, the server's `porsche/hd` set).
//! * **Logical size stays the original's**: layout keeps using the entry's width and height; only the uploaded
//!   pixels are bigger (UVs are normalised). An HD texture skips the original's 16-bit cache reduction.
//! * Car pages (entries blitted at pixel positions) and menu art are not routed here yet: they stay original.
//! * `NFS_HD_DUMP=1` writes `hd-used.csv` in the data folder: every entry the game uploaded through the store (key,
//!   source, name, size) — the coverage the offline scan can't see.

use crate::texture::TextureImage;
use nfs_renderer::{Renderer, TexFormat, TextureId};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// One HD texture: the backing pixels of an entry at `scale`× its size.
#[derive(Debug)]
pub struct HdImage {
    pub width: u32,
    pub height: u32,
    pub format: TexFormat,
    /// level 0 first
    pub levels: Vec<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    file: String,
}

#[derive(Debug, Deserialize)]
struct Index {
    format: u32,
    #[serde(default = "two")]
    scale: u32,
    entries: BTreeMap<String, IndexEntry>,
}

fn two() -> u32 {
    2
}

struct Store {
    dir: PathBuf,
    scale: u32,
    entries: BTreeMap<String, IndexEntry>,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static LOOKUPS: AtomicU32 = AtomicU32::new(0);
static HITS: AtomicU32 = AtomicU32::new(0);

/// Where a pack is looked for.
pub fn pack_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("NFS_HD_PATH").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    if cfg!(target_os = "tvos") {
        // beside the downloaded game data: the server's `porsche/hd` set
        return crate::platform::tvos_caches().join("games/porsche/hd");
    }
    crate::platform::data_dir().join("hd")
}

fn store() -> Option<&'static Store> {
    static S: OnceLock<Option<Store>> = OnceLock::new();
    S.get_or_init(|| {
        let dir = pack_dir();
        let text = std::fs::read_to_string(dir.join("index.json")).ok()?;
        match serde_json::from_str::<Index>(&text) {
            Ok(ix) if ix.format == 1 && matches!(ix.scale, 2 | 4) => {
                tracing::info!(dir = %dir.display(), entries = ix.entries.len(), scale = ix.scale, "HD texture pack found");
                Some(Store { dir, scale: ix.scale, entries: ix.entries })
            }
            Ok(ix) => {
                tracing::warn!(format = ix.format, scale = ix.scale, "HD texture pack: unknown format or scale, ignored");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "HD texture pack: index.json unreadable, ignored");
                None
            }
        }
    })
    .as_ref()
}

/// Whether a pack is installed (whatever the setting).
pub fn available() -> bool {
    store().is_some()
}

/// Extras › Graphics › Textures: HD on or off (applies to textures loaded from now on).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn active() -> bool {
    ENABLED.load(Ordering::Relaxed) && store().is_some()
}

/// Whether anything needs entry keys (HD on with a pack, or the dump).
pub fn wanted() -> bool {
    active() || dump_on()
}

/// The key an entry's HD texture is filed under: the first 16 bytes of the SHA-256 of its raw bytes, in hex.
pub fn entry_key(raw: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(raw).iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// Textures uploaded through the store / of them HD, since [`reset_counts`] (the frame overlay shows them).
pub fn counts() -> (u32, u32) {
    (HITS.load(Ordering::Relaxed), LOOKUPS.load(Ordering::Relaxed))
}

pub fn reset_counts() {
    HITS.store(0, Ordering::Relaxed);
    LOOKUPS.store(0, Ordering::Relaxed);
}

fn dump_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NFS_HD_DUMP").is_ok_and(|v| v != "0"))
}

/// `NFS_HD_DUMP`: an entry seen (once per key per run).
fn dump(key: &str, source: &Path, name: &str, w: u32, h: u32) {
    static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let Ok(mut seen) = SEEN.lock() else { return };
    let seen = seen.get_or_insert_with(HashSet::new);
    if !seen.insert(key.to_string()) {
        return;
    }
    let path = crate::platform::data_dir().join("hd-used.csv");
    let new = !path.exists();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        if new {
            let _ = writeln!(f, "hash,source,entry,width,height");
        }
        let _ = writeln!(f, "{key},{},{},{w},{h}", source.display().to_string().replace(',', ";"), name.trim().replace(',', ";"));
    }
}

/// The HD texture for an entry, if the pack has one (and HD is on). `levels` = how many levels the engine uses for
/// it (the chain is cut to that).
pub fn lookup(raw: &[u8], source: &Path, name: &str, w: u32, h: u32, levels: usize) -> Option<Arc<HdImage>> {
    if !wanted() {
        return None;
    }
    let key = entry_key(raw);
    if dump_on() {
        dump(&key, source, name, w, h);
    }
    if !active() {
        return None;
    }
    let st = store()?;
    let e = st.entries.get(&key)?;
    let img = match read_ktx2(&st.dir.join(&e.file)) {
        Ok(img) => img,
        Err(err) => {
            tracing::warn!(file = %e.file, error = %err, "HD texture unreadable: original used");
            return None;
        }
    };
    if img.width != w * st.scale || img.height != h * st.scale {
        tracing::warn!(file = %e.file, w, h, hd_w = img.width, hd_h = img.height, "HD texture is not exactly the pack's scale: original used");
        return None;
    }
    let mut img = img;
    img.levels.truncate(levels.max(1));
    Some(Arc::new(img))
}

/// What to upload for `img`: its HD texture (as it is), else its own levels, through the original's 16-bit cache
/// when `cache` (the packs the original keeps in 16 bits, `TextureImage::cache_mips`).
pub fn gpu_levels(img: &TextureImage, cache: bool) -> (u32, u32, TexFormat, std::borrow::Cow<'_, [Vec<u8>]>) {
    LOOKUPS.fetch_add(1, Ordering::Relaxed);
    // ASTC only where the device samples it (decided here, at upload: a track can be decoded on a worker thread
    // before the device exists); otherwise the original
    let hd = img.hd.as_deref().filter(|h| h.format == TexFormat::Rgba8 || nfs_renderer::astc_supported());
    if hd.is_some() {
        HITS.fetch_add(1, Ordering::Relaxed);
    }
    match hd {
        Some(hd) => (hd.width, hd.height, hd.format, std::borrow::Cow::Borrowed(hd.levels.as_slice())),
        None if cache => (img.width, img.height, TexFormat::Rgba8, std::borrow::Cow::Owned(img.cache_mips())),
        None => (img.width, img.height, TexFormat::Rgba8, std::borrow::Cow::Borrowed(img.mips.as_slice())),
    }
}

/// Upload `img` ([`gpu_levels`]).
pub fn upload(r: &mut Renderer, img: &TextureImage, cache: bool) -> TextureId {
    let (w, h, f, levels) = gpu_levels(img, cache);
    r.upload_texture_fmt(w, h, f, &levels)
}

// ---- KTX2 (the subset a pack uses: one 2-D image, no supercompression) ----

const KTX2_ID: [u8; 12] = [0xAB, b'K', b'T', b'X', b' ', b'2', b'0', 0xBB, b'\r', b'\n', 0x1A, b'\n'];
/// `VK_FORMAT_R8G8B8A8_UNORM`, `VK_FORMAT_ASTC_4x4_UNORM_BLOCK`
pub const VK_RGBA8: u32 = 37;
pub const VK_ASTC_4X4: u32 = 157;

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?))
}

/// Read a pack's KTX2 file.
pub fn read_ktx2(path: &Path) -> Result<HdImage, String> {
    let d = std::fs::read(path).map_err(|e| e.to_string())?;
    parse_ktx2(&d)
}

pub fn parse_ktx2(d: &[u8]) -> Result<HdImage, String> {
    if d.get(..12) != Some(&KTX2_ID[..]) {
        return Err("not a KTX2 file".into());
    }
    let bad = || "truncated KTX2 header".to_string();
    let vk = u32_at(d, 12).ok_or_else(bad)?;
    let (width, height) = (u32_at(d, 20).ok_or_else(bad)?, u32_at(d, 24).ok_or_else(bad)?);
    let levels = u32_at(d, 40).ok_or_else(bad)?.max(1) as usize;
    if u32_at(d, 44).ok_or_else(bad)? != 0 {
        return Err("supercompressed KTX2 is not supported".into());
    }
    let format = match vk {
        VK_RGBA8 => TexFormat::Rgba8,
        VK_ASTC_4X4 => TexFormat::Astc4x4,
        other => return Err(format!("vkFormat {other} is not supported (RGBA8 or ASTC 4x4)")),
    };
    let mut out = Vec::with_capacity(levels);
    let (mut w, mut h) = (width, height);
    for l in 0..levels {
        let o = 80 + l * 24;
        let (off, len) = (u64_at(d, o).ok_or_else(bad)? as usize, u64_at(d, o + 8).ok_or_else(bad)? as usize);
        let (row, rows) = format.layout(w, h);
        if len != (row * rows) as usize {
            return Err(format!("level {l}: {len} bytes, expected {}", row * rows));
        }
        out.push(d.get(off..off + len).ok_or_else(|| format!("level {l} past the end"))?.to_vec());
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    Ok(HdImage { width, height, format, levels: out })
}

/// Write a KTX2 file (the pack tool): `levels` level 0 first, each `format.layout` bytes.
pub fn write_ktx2(width: u32, height: u32, format: TexFormat, levels: &[Vec<u8>]) -> Vec<u8> {
    let vk = match format {
        TexFormat::Rgba8 => VK_RGBA8,
        TexFormat::Astc4x4 => VK_ASTC_4X4,
    };
    let type_size = if format == TexFormat::Rgba8 { 1 } else { 1 };
    let mut h = Vec::new();
    h.extend_from_slice(&KTX2_ID);
    for v in [vk, type_size, width, height, 0, 0, 1, levels.len() as u32, 0] {
        h.extend_from_slice(&v.to_le_bytes());
    }
    // dfd / kvd / sgd: none (a pack's files are read by this engine only)
    h.extend_from_slice(&[0u8; 32]);
    let index_at = h.len();
    h.resize(index_at + 24 * levels.len(), 0);
    // level data, smallest level last here (any order is valid: the index says where each is)
    let mut data_at = h.len().next_multiple_of(16);
    h.resize(data_at, 0);
    for (l, lv) in levels.iter().enumerate() {
        let o = index_at + l * 24;
        h[o..o + 8].copy_from_slice(&(data_at as u64).to_le_bytes());
        h[o + 8..o + 16].copy_from_slice(&(lv.len() as u64).to_le_bytes());
        h[o + 16..o + 24].copy_from_slice(&(lv.len() as u64).to_le_bytes());
        h.extend_from_slice(lv);
        data_at = h.len().next_multiple_of(16);
        h.resize(data_at, 0);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ktx2_round_trip() {
        let l0 = vec![7u8; 8 * 4 * 4];
        let l1 = vec![9u8; 4 * 2 * 4];
        let file = write_ktx2(8, 4, TexFormat::Rgba8, &[l0.clone(), l1.clone()]);
        let img = parse_ktx2(&file).unwrap();
        assert_eq!((img.width, img.height, img.format), (8, 4, TexFormat::Rgba8));
        assert_eq!(img.levels, vec![l0, l1]);
        // ASTC 4x4: 16 bytes per block, a 6x6 level rounds up to 2x2 blocks
        let a = write_ktx2(6, 6, TexFormat::Astc4x4, &[vec![1u8; 64]]);
        assert_eq!(parse_ktx2(&a).unwrap().levels[0].len(), 64);
        assert!(parse_ktx2(&a[..40]).is_err());
        assert!(parse_ktx2(b"nope").is_err());
    }

    #[test]
    fn the_key_is_a_stable_hex_digest() {
        assert_eq!(entry_key(b"abc"), "ba7816bf8f01cfea414140de5dae2223");
    }
}
