//! Is an `NFS_DOS.EXE` one of the two builds ReNFS runs? Ported from ReNFS's `port/src/exe.c` (GPL-3.0, Danik):
//! the LE image's pages are placed at their addresses, the 32-bit offset fix-ups applied, and the FNV-1a of the
//! first 0x169a10 bytes compared with the two known sums. Only reads the file.

const IMAGE_LEN: usize = 0x169a10;
const MAX_PAGES: usize = 1024;
/// 19 June 1996 and 17 June 1996 (the 1997 EA CD), both 1,254,591 bytes (`exe.c`)
pub const IMAGE_SUMS: [u32; 2] = [0x76078ed2, 0x21ef7c33];

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn u16_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?) as u32)
}

fn fnv(p: &[u8]) -> u32 {
    p.iter().fold(2166136261u32, |h, &b| (h ^ b as u32).wrapping_mul(16777619))
}

/// The loaded image's FNV-1a, or why it isn't an LE program ReNFS can load.
pub fn image_sum(d: &[u8]) -> Result<u32, String> {
    let le = if d.len() > 0x40 { u32_at(d, 0x3c).unwrap_or(0) as usize } else { 0 };
    if d.len() < 2 || &d[..2] != b"MZ" || le + 0xb0 > d.len() || &d[le..le + 2] != b"LE" {
        return Err("not the game's program (no MZ / LE header)".into());
    }
    let g = |o: usize| u32_at(d, le + o).ok_or_else(|| "truncated LE header".to_string());
    let npages = g(0x14)? as usize;
    let objtab = le + g(0x40)? as usize;
    let nobj = g(0x44)? as usize;
    let datapages = g(0x80)? as usize;
    if nobj > 7 || npages > MAX_PAGES || g(0x28)? != 4096 {
        return Err("unexpected LE layout".into());
    }
    let mut img = vec![0u8; IMAGE_LEN];
    let mut page_va = vec![0u32; MAX_PAGES];
    let mut base = [0u32; 8];
    for i in 0..nobj {
        let o = objtab + i * 24;
        let b = u32_at(d, o + 4).ok_or("truncated object table")?;
        let first = u32_at(d, o + 0xc).ok_or("truncated object table")? as usize;
        let n = u32_at(d, o + 0x10).ok_or("truncated object table")? as usize;
        base[i + 1] = b;
        for p in 0..n {
            let page = (first + p).wrapping_sub(1);
            let at = datapages + page * 4096;
            let va = b as usize + p * 4096;
            if page >= npages || va + 4096 > IMAGE_LEN + 4096 {
                return Err("page outside the image".into());
            }
            page_va[page] = va as u32;
            if at < d.len() {
                let mut k = if at + 4096 <= d.len() { 4096 } else { d.len() - at };
                if va + k > IMAGE_LEN {
                    k = IMAGE_LEN.saturating_sub(va);
                }
                img[va..va + k].copy_from_slice(&d[at..at + k]);
            }
        }
    }
    let fpt = le + g(0x68)? as usize;
    let frt = le + g(0x6c)? as usize;
    for page in 0..npages {
        let mut o = frt + u32_at(d, fpt + 4 * page).ok_or("truncated fix-ups")? as usize;
        let end = frt + u32_at(d, fpt + 4 * page + 4).ok_or("truncated fix-ups")? as usize;
        while o < end && end <= d.len() {
            let src = d[o] as u32;
            let flags = d[o + 1] as u32;
            o += 2;
            if flags & 3 != 0 {
                return Err("a fix-up kind this program doesn't have".into());
            }
            let mut n = 1usize;
            let mut one = 0i32;
            if src & 0x20 != 0 {
                n = d[o] as usize;
                o += 1;
            } else {
                one = u16_at(d, o).ok_or("truncated")? as u16 as i16 as i32;
                o += 2;
            }
            let obj = if flags & 0x40 != 0 {
                let v = u16_at(d, o).ok_or("truncated")?;
                o += 2;
                v
            } else {
                let v = d[o] as u32;
                o += 1;
                v
            };
            let mut target = 0u32;
            if src & 0x0f != 2 {
                if flags & 0x10 != 0 {
                    target = u32_at(d, o).ok_or("truncated")?;
                    o += 4;
                } else {
                    target = u16_at(d, o).ok_or("truncated")?;
                    o += 2;
                }
            }
            let list = o;
            if src & 0x20 != 0 {
                o += 2 * n;
            }
            if src & 0x0f != 7 || obj > 7 {
                continue;
            }
            for k in 0..n {
                let at = if src & 0x20 != 0 { u16_at(d, list + 2 * k).ok_or("truncated")? as u16 as i16 as i32 } else { one };
                let va = page_va[page] as i64 + at as i64;
                let v = base[obj as usize].wrapping_add(target);
                if at < 0 || va < 0 || va as usize + 4 > IMAGE_LEN {
                    continue;
                }
                img[va as usize..va as usize + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
    Ok(fnv(&img))
}

/// Whether ReNFS accepts this `NFS_DOS.EXE`; the sum either way.
pub fn renfs_accepts(d: &[u8]) -> Result<(bool, u32), String> {
    image_sum(d).map(|s| (IMAGE_SUMS.contains(&s), s))
}
