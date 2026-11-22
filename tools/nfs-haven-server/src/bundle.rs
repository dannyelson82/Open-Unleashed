//! **Fast install bundle**: `GET /v1/data/{game}/{set}.tar` — the whole set as one uncompressed tar, made on the fly
//! in manifest order from the files themselves (nothing extra on disk). The layout is fully determined by the
//! manifest (ustar headers with fixed mode / owner / time 0, each file padded to 512 bytes, two zero blocks at the
//! end), so every byte's place is known in advance and `Range` works like on any file: an app resumes a fresh
//! install from the first file it doesn't have yet. The `ETag` is the manifest's hash: a range is only honoured
//! (`If-Range`) for the same manifest. The app verifies every file's SHA-256 from the manifest as it extracts.

use crate::{Shared, error};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use nfs_haven_manifest::Manifest;
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const BLOCK: u64 = 512;

fn padded(size: u64) -> u64 {
    size.div_ceil(BLOCK) * BLOCK
}

/// The ustar header of a regular file: `path` (split into prefix / name when over 100 bytes), `size`; mode 0644,
/// owner 0, time 0. `None` when the path doesn't fit ustar (over 255 bytes, or no split point).
pub fn header(path: &str, size: u64) -> Option<[u8; 512]> {
    let p = path.as_bytes();
    let (prefix, name) = if p.len() <= 100 {
        (&p[..0], p)
    } else {
        // the last '/' that leaves a name of ≤ 100 bytes and a prefix of ≤ 155
        let cut = (0..p.len()).rev().find(|&i| p[i] == b'/' && p.len() - i - 1 <= 100 && i <= 155)?;
        (&p[..cut], &p[cut + 1..])
    };
    if size >= 1 << 33 {
        return None; // 11 octal digits
    }
    let mut h = [0u8; 512];
    h[..name.len()].copy_from_slice(name);
    h[100..107].copy_from_slice(b"0000644");
    h[108..115].copy_from_slice(b"0000000");
    h[116..123].copy_from_slice(b"0000000");
    h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
    h[136..147].copy_from_slice(b"00000000000");
    h[148..156].copy_from_slice(b"        ");
    h[156] = b'0';
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[345..345 + prefix.len()].copy_from_slice(prefix);
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
    Some(h)
}

/// Where each entry's header starts, and the tar's total length.
pub struct Layout {
    pub offsets: Vec<u64>,
    pub total: u64,
}

pub fn layout(m: &Manifest) -> Layout {
    let mut offsets = Vec::with_capacity(m.files.len());
    let mut at = 0u64;
    for f in &m.files {
        offsets.push(at);
        at += BLOCK + padded(f.size);
    }
    Layout { offsets, total: at + 2 * BLOCK }
}

/// What lies in the tar at `[start, end]` (inclusive), in order.
enum Piece {
    Header { entry: usize, from: usize, to: usize },
    Data { entry: usize, from: u64, len: u64 },
    Zeros(u64),
}

fn pieces(m: &Manifest, lay: &Layout, start: u64, end: u64) -> Vec<Piece> {
    let mut out = Vec::new();
    let clip = |a: u64, b: u64| (a.max(start), b.min(end + 1));
    // the first entry the range touches
    let first = lay.offsets.partition_point(|&o| o <= start).saturating_sub(1);
    for (i, f) in m.files.iter().enumerate().skip(first) {
        let o = lay.offsets[i];
        if o > end {
            break;
        }
        let (a, b) = clip(o, o + BLOCK);
        if a < b {
            out.push(Piece::Header { entry: i, from: (a - o) as usize, to: (b - o) as usize });
        }
        let d = o + BLOCK;
        let (a, b) = clip(d, d + f.size);
        if a < b {
            out.push(Piece::Data { entry: i, from: a - d, len: b - a });
        }
        let (a, b) = clip(d + f.size, d + padded(f.size));
        if a < b {
            out.push(Piece::Zeros(b - a));
        }
    }
    let tail = lay.total - 2 * BLOCK;
    let (a, b) = clip(tail, lay.total);
    if a < b {
        out.push(Piece::Zeros(b - a));
    }
    out
}

/// The `ETag` of a set's tar: from the manifest's own bytes.
fn etag(json: &str) -> String {
    let d = sha2::Sha256::digest(json.as_bytes());
    format!("\"tar-{}\"", d.iter().take(16).map(|b| format!("{b:02x}")).collect::<String>())
}

/// `GET /v1/data/{game}/{set}.tar`
pub async fn tar(State(st): State<Shared>, Path((game, file)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let Some(set) = file.strip_suffix(".tar") else { return error(StatusCode::NOT_FOUND, "not found") };
    let id = format!("{game}/{set}");
    let Some(b) = st.data.get(&id) else { return error(StatusCode::NOT_FOUND, format!("no data set {id:?}")) };
    if let Some(bad) = b.manifest.files.iter().find(|f| header(&f.path, f.size).is_none()) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, format!("{} does not fit a tar header", bad.path));
    }
    let lay = layout(&b.manifest);
    let tag = etag(&b.json);
    let range_ok = headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()).is_none_or(|v| v == tag);
    let range = if range_ok { headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| crate::data::parse_range(v, lay.total)) } else { None };
    let mut h = HeaderMap::new();
    h.insert(header::ETAG, HeaderValue::from_str(&tag).unwrap());
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-tar"));
    let (status, start, end) = match range {
        Some(Err(())) => {
            h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{}", lay.total)).unwrap());
            return (StatusCode::RANGE_NOT_SATISFIABLE, h).into_response();
        }
        Some(Ok((s, e))) => {
            h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {s}-{e}/{}", lay.total)).unwrap());
            (StatusCode::PARTIAL_CONTENT, s, e)
        }
        None => (StatusCode::OK, 0, lay.total - 1),
    };
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(end - start + 1));
    let plan = pieces(&b.manifest, &lay, start, end);
    let built = b.clone();
    let (mut w, r) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
        if let Err(e) = write_pieces(&built.manifest, &built.sources, &plan, &mut w).await {
            tracing::warn!(set = %built.manifest.set, error = %e, "tar stream ended early");
        }
        let _ = w.shutdown().await;
    });
    (status, h, Body::from_stream(tokio_util::io::ReaderStream::with_capacity(r, 256 * 1024))).into_response()
}

async fn write_pieces(m: &Manifest, sources: &BTreeMap<String, PathBuf>, plan: &[Piece], w: &mut tokio::io::DuplexStream) -> std::io::Result<()> {
    let zeros = [0u8; 512];
    for p in plan {
        match *p {
            Piece::Header { entry, from, to } => {
                let f = &m.files[entry];
                let h = header(&f.path, f.size).expect("checked");
                w.write_all(&h[from..to]).await?;
            }
            Piece::Data { entry, from, len } => {
                let f = &m.files[entry];
                let src = sources.get(&f.path).ok_or_else(|| std::io::Error::other("no source"))?;
                let mut file = tokio::fs::File::open(src).await?;
                if file.metadata().await?.len() != f.size {
                    return Err(std::io::Error::other(format!("{} changed since the manifest was built", f.path)));
                }
                file.seek(std::io::SeekFrom::Start(from)).await?;
                let n = tokio::io::copy(&mut file.take(len), w).await?;
                if n != len {
                    return Err(std::io::Error::other(format!("{} shorter than its manifest entry", f.path)));
                }
            }
            Piece::Zeros(mut n) => {
                while n > 0 {
                    let k = n.min(512) as usize;
                    w.write_all(&zeros[..k]).await?;
                    n -= k as u64;
                }
            }
        }
    }
    Ok(())
}
