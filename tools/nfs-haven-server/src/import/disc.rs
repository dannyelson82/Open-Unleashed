//! Disc images, read in place (nothing converted on disk): a `.cue` sheet's data track, a raw image of 2352-byte
//! sectors (`.bin`, `.img`: the 16-byte sync header found at the start), or a plain 2048-byte ISO. The file system is
//! ISO 9660, through its Joliet tree when the disc has one (mixed-case and long names, as Windows shows them).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const SYNC: [u8; 12] = [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0];

/// One file on a disc.
#[derive(Debug, Clone)]
pub struct Entry {
    /// relative path with `/`, as the disc spells it
    pub path: String,
    pub lba: u32,
    pub size: u64,
}

pub struct Disc {
    file: File,
    /// bytes per sector in the image (2048 or 2352) and where a sector's 2048 bytes of data start
    sector: u64,
    data_at: u64,
    pub volume: String,
    pub joliet: bool,
    root: (u32, u32),
}

/// The data file a `.cue` sheet names for its first track, and that track's sector size.
fn parse_cue(cue: &Path) -> Result<(PathBuf, u64), String> {
    let text = std::fs::read_to_string(cue).map_err(|e| format!("{}: {e}", cue.display()))?;
    let mut file = None;
    let mut sector = None;
    for line in text.lines() {
        let l = line.trim();
        if file.is_none() && l.to_ascii_uppercase().starts_with("FILE ") {
            let name = l[5..].trim();
            let name = if let Some(rest) = name.strip_prefix('"') { rest.split('"').next().unwrap_or("") } else { name.split_whitespace().next().unwrap_or("") };
            file = Some(cue.with_file_name(name));
        }
        if sector.is_none() && l.to_ascii_uppercase().starts_with("TRACK ") {
            let up = l.to_ascii_uppercase();
            sector = Some(if up.contains("/2048") { 2048 } else if up.contains("MODE1/2352") || up.contains("MODE2/2352") { 2352 } else { return Err(format!("first track is not a data track: {l}")) });
        }
    }
    Ok((file.ok_or("no FILE line")?, sector.ok_or("no TRACK line")?))
}

/// The data file of an image (a cue's `.bin`, or the image itself).
pub fn data_file(path: &Path) -> Result<PathBuf, String> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue")) { parse_cue(path).map(|(f, _)| f) } else { Ok(path.to_path_buf()) }
}

impl Disc {
    pub fn open(path: &Path) -> Result<Disc, String> {
        let (data, cue_sector) = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue")) {
            let (f, s) = parse_cue(path)?;
            (f, Some(s))
        } else {
            (path.to_path_buf(), None)
        };
        let mut file = File::open(&data).map_err(|e| format!("{}: {e}", data.display()))?;
        let mut head = [0u8; 12];
        file.read_exact(&mut head).map_err(|e| format!("{}: {e}", data.display()))?;
        let sector = cue_sector.unwrap_or(if head == SYNC { 2352 } else { 2048 });
        // MODE1/2352: 12 sync + 4 header; MODE2 form 1 would be 24 (sub-header too)
        let data_at = if sector == 2352 { 16 } else { 0 };
        let mut d = Disc { file, sector, data_at, volume: String::new(), joliet: false, root: (0, 0) };
        let mut primary = None;
        let mut joliet = None;
        for n in 16..64 {
            let s = d.sector_data(n)?;
            if &s[1..6] != b"CD001" {
                if n == 16 { return Err("no ISO 9660 file system (CD001) at sector 16".into()) }
                break;
            }
            let root = (u32::from_le_bytes(s[158..162].try_into().unwrap()), u32::from_le_bytes(s[166..170].try_into().unwrap()));
            match s[0] {
                1 if primary.is_none() => primary = Some((root, String::from_utf8_lossy(&s[40..72]).trim().to_string())),
                2 if matches!(&s[88..91], b"%/@" | b"%/C" | b"%/E") => joliet = Some((root, ucs2(&s[40..72]).trim_matches(['\0', ' ']).to_string())),
                255 => break,
                _ => {}
            }
        }
        let ((root, volume), j) = match (joliet, primary) {
            (Some(j), _) => (j, true),
            (None, Some(p)) => (p, false),
            _ => return Err("no primary volume descriptor".into()),
        };
        d.root = root;
        d.volume = volume;
        d.joliet = j;
        Ok(d)
    }

    fn sector_data(&mut self, lba: u32) -> Result<[u8; 2048], String> {
        let mut b = [0u8; 2048];
        self.file.seek(SeekFrom::Start(lba as u64 * self.sector + self.data_at)).map_err(|e| e.to_string())?;
        self.file.read_exact(&mut b).map_err(|e| format!("sector {lba}: {e}"))?;
        Ok(b)
    }

    /// Every file on the disc, depth first, sorted by path.
    pub fn files(&mut self) -> Result<Vec<Entry>, String> {
        let mut out = Vec::new();
        let mut stack = vec![(String::new(), self.root, 0usize)];
        while let Some((dir, (lba, len), depth)) = stack.pop() {
            if depth > 32 {
                return Err("directories nested too deep".into());
            }
            let sectors = (len as u64).div_ceil(2048) as u32;
            for k in 0..sectors {
                let s = self.sector_data(lba + k)?;
                let mut o = 0usize;
                while o < 2048 {
                    let rl = s[o] as usize;
                    if rl == 0 {
                        break; // the rest of this sector is padding
                    }
                    if o + rl > 2048 || rl < 34 {
                        return Err(format!("bad directory record in sector {}", lba + k));
                    }
                    let r = &s[o..o + rl];
                    o += rl;
                    let nl = r[32] as usize;
                    let raw = &r[33..33 + nl.min(rl - 33)];
                    if raw == [0] || raw == [1] {
                        continue; // . and ..
                    }
                    let mut name = if self.joliet { ucs2(raw) } else { String::from_utf8_lossy(raw).into_owned() };
                    if let Some(i) = name.rfind(';') {
                        name.truncate(i);
                    }
                    if !self.joliet && name.ends_with('.') {
                        name.pop();
                    }
                    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                        return Err(format!("unusable name on the disc: {name:?}"));
                    }
                    let path = if dir.is_empty() { name } else { format!("{dir}/{name}") };
                    let extent = u32::from_le_bytes(r[2..6].try_into().unwrap());
                    let size = u32::from_le_bytes(r[10..14].try_into().unwrap());
                    if r[25] & 2 != 0 {
                        stack.push((path, (extent, size), depth + 1));
                    } else if r[25] & 0x80 != 0 {
                        return Err(format!("{path}: multi-extent files are not supported"));
                    } else {
                        out.push(Entry { path, lba: extent, size: size as u64 });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Copy one file's bytes to `w`.
    pub fn copy(&mut self, e: &Entry, w: &mut impl Write) -> Result<(), String> {
        let mut left = e.size;
        let mut lba = e.lba;
        let mut buf = vec![0u8; 2048];
        while left > 0 {
            self.file.seek(SeekFrom::Start(lba as u64 * self.sector + self.data_at)).map_err(|er| er.to_string())?;
            let n = left.min(2048) as usize;
            self.file.read_exact(&mut buf[..n]).map_err(|er| format!("{}: {er}", e.path))?;
            w.write_all(&buf[..n]).map_err(|er| er.to_string())?;
            left -= n as u64;
            lba += 1;
        }
        Ok(())
    }

    /// One file's bytes (small files).
    pub fn read(&mut self, e: &Entry) -> Result<Vec<u8>, String> {
        let mut v = Vec::with_capacity(e.size as usize);
        self.copy(e, &mut v)?;
        Ok(v)
    }
}

fn ucs2(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u)
}

/// A tiny ISO 9660 image (tests): files at the root and one level of folders, written as raw 2352-byte sectors when
/// `raw`, with a Joliet tree.
#[cfg(test)]
pub fn make_test_image(files: &[(&str, &[u8])], raw: bool) -> Vec<u8> {
    // layout: 16 system sectors, PVD 16, SVD 17, terminator 18, then dirs and files
    let mut dirs: Vec<String> = vec![String::new()];
    for (p, _) in files {
        if let Some((d, _)) = p.rsplit_once('/') {
            if !dirs.contains(&d.to_string()) {
                dirs.push(d.to_string());
            }
        }
    }
    let mut next = 19u32;
    let dir_lba: Vec<u32> = dirs.iter().map(|_| { let l = next; next += 1; l }).collect();
    let file_lba: Vec<u32> = files.iter().map(|(_, d)| { let l = next; next += (d.len() as u32).div_ceil(2048).max(1); l }).collect();
    let total = next;
    let mut img = vec![0u8; total as usize * 2048];
    let rec = |name: &[u8], lba: u32, size: u32, dir: bool| -> Vec<u8> {
        let mut r = vec![0u8; 33 + name.len() + (1 - name.len() % 2)];
        r[0] = r.len() as u8;
        r[2..6].copy_from_slice(&lba.to_le_bytes());
        r[10..14].copy_from_slice(&size.to_le_bytes());
        r[25] = if dir { 2 } else { 0 };
        r[32] = name.len() as u8;
        r[33..33 + name.len()].copy_from_slice(name);
        r
    };
    let j = |s: &str| s.encode_utf16().flat_map(|c| c.to_be_bytes()).collect::<Vec<u8>>();
    for (di, d) in dirs.iter().enumerate() {
        let mut body = rec(&[0], dir_lba[di], 2048, true);
        body.extend(rec(&[1], dir_lba[0], 2048, true));
        for (dj, sub) in dirs.iter().enumerate() {
            if dj > 0 && di == 0 {
                body.extend(rec(&j(sub), dir_lba[dj], 2048, true));
            }
        }
        for (fi, (p, data)) in files.iter().enumerate() {
            let (pd, name) = p.rsplit_once('/').unwrap_or(("", p));
            if pd == d {
                body.extend(rec(&j(&format!("{name};1")), file_lba[fi], data.len() as u32, false));
            }
        }
        let at = dir_lba[di] as usize * 2048;
        img[at..at + body.len()].copy_from_slice(&body);
    }
    for (fi, (_, data)) in files.iter().enumerate() {
        let at = file_lba[fi] as usize * 2048;
        img[at..at + data.len()].copy_from_slice(data);
    }
    for (n, t) in [(16usize, 1u8), (17, 2), (18, 255)] {
        let s = &mut img[n * 2048..n * 2048 + 2048];
        s[0] = t;
        s[1..6].copy_from_slice(b"CD001");
        s[6] = 1;
        if t != 255 {
            if t == 2 {
                s[40..48].copy_from_slice(&[0, b'T', 0, b'E', 0, b'S', 0, b'T']);
            } else {
                s[40..44].copy_from_slice(b"TEST");
            }
            let root = rec(&[0], dir_lba[0], 2048, true);
            s[156..156 + 34].copy_from_slice(&root[..34]);
        }
        if t == 2 {
            s[88..91].copy_from_slice(b"%/E");
        }
    }
    if !raw {
        return img;
    }
    let mut out = Vec::with_capacity(total as usize * 2352);
    for s in img.chunks(2048) {
        out.extend_from_slice(&SYNC);
        out.extend_from_slice(&[0, 2, 0, 1]);
        out.extend_from_slice(s);
        out.extend_from_slice(&[0u8; 288]);
    }
    out
}
