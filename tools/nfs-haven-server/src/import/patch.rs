//! Known game updates, recognised by their checksum and unpacked **without running them**. An update is a table of
//! the files it installs, each with the SHA-256 it must have: the installer's compressed streams are found by trying
//! a raw deflate at every offset, and a stream is kept only when its output is one of the files listed (so neither
//! the installer's own format nor its stream order matters, and nothing unexpected can come out).

use sha2::Digest;

/// One file an update installs.
pub struct PatchFile {
    pub path: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub struct Patch {
    pub game: &'static str,
    pub name: &'static str,
    /// SHA-256 of the installer, and of the download it may come in (a `.zip` holding it)
    pub installer_sha256: &'static str,
    pub download_sha256: &'static [&'static str],
    pub files: &'static [PatchFile],
}

/// Need for Speed: Porsche Unleashed, "Enhanced Version 3.5.20040310" (a Wise installer, `SETUP.EXE`): the data files
/// it replaces and its game executable — installed as `nfs5.exe`, the file the engine reads the intro movies' decoder
/// tables from (never run). `gimme.dll`, the uninstaller, the readme and the per-profile HUD copies are left out.
pub const PORSCHE_35: Patch = Patch {
    game: "porsche",
    name: "Porsche Unleashed update 3.5",
    installer_sha256: "3da6bc8639f7d5c484033c88cf99cbdbeb3fda572e5531698b06520043565e29",
    download_sha256: &["5ac3faa0772d393cb06e71e18d00bce64d159f0d043afbf99f74e50fced859e6"],
    files: &[
        PatchFile { path: "version.txt", sha256: "8a199b120cf400d69c300e1dd6eacdc8b0002353e3044bbab21e0125b32c9fd6", size: 3 },
        PatchFile { path: "properties.txt", sha256: "e383fd5d672b4da32dbdf66942d21d1e3499ea0503396c7ea8fdf4a7d7f05077", size: 403 },
        PatchFile { path: "FEData/Layouts/garage.lay", sha256: "53bfe5c199dd81eab04fea93f4a1a32f9d6fe519768420497f9d26875521090a", size: 7847 },
        PatchFile { path: "FEData/Layouts/loadreplay.lay", sha256: "c0d9240ce57cd9f232303cce37e4adf47ca5ea3e6ed202ca1600810fb330be9a", size: 5544 },
        PatchFile { path: "FEData/Layouts/usedcars.lay", sha256: "c18d46669e0bac070e4ae30363e8c1a704f86bda2c2d8631fc892a49cd45990b", size: 6160 },
        PatchFile { path: "FEData/Data/nfs5-1.hud", sha256: "affd01482d4d0afe67780ea3e02ce9b24daec9557c350d120cb8e26d1762d5cb", size: 2400 },
        PatchFile { path: "FEData/Data/nfs5-2.hud", sha256: "7e074cd6ed7838a128df0c1da2e8d1f484b401607dda94582243bdf71c9e3427", size: 520 },
        PatchFile { path: "FEData/Locale/festrings.loc", sha256: "7c0bffe19f926025ee25c948514e24e86c2f7394853298322b60645448539c9a", size: 608224 },
        PatchFile { path: "nfs5.exe", sha256: "02437ddbf41af86dffe14251b28ed8889bd1c4c904fe77ff730d8912257ec90d", size: 2129965 },
    ],
};

pub const KNOWN: &[&Patch] = &[&PORSCHE_35];

pub fn sha256_hex(b: &[u8]) -> String {
    sha2::Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// Which known update a file is (by checksum), and the installer's bytes: the file itself, or its entry in a `.zip`.
pub fn recognise(bytes: &[u8]) -> Option<(&'static Patch, Vec<u8>)> {
    let sha = sha256_hex(bytes);
    for p in KNOWN {
        if sha == p.installer_sha256 {
            return Some((p, bytes.to_vec()));
        }
    }
    if bytes.starts_with(b"PK\x03\x04") {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
        for i in 0..z.len() {
            let Ok(mut f) = z.by_index(i) else { continue };
            if f.size() > 64 << 20 {
                continue;
            }
            let mut inner = Vec::with_capacity(f.size() as usize);
            if std::io::Read::read_to_end(&mut f, &mut inner).is_err() {
                continue;
            }
            let s = sha256_hex(&inner);
            if let Some(p) = KNOWN.iter().find(|p| p.installer_sha256 == s) {
                return Some((p, inner));
            }
        }
    }
    None
}

/// Every file of `p` out of its installer: `(path, bytes)` in the table's order. An error names what is missing.
pub fn unpack(p: &Patch, installer: &[u8]) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let max = p.files.iter().map(|f| f.size).max().unwrap_or(0) as usize;
    let mut found: Vec<Option<Vec<u8>>> = vec![None; p.files.len()];
    let mut out = vec![0u8; max + 1];
    let mut i = 0usize;
    while i + 2 < installer.len() {
        // a deflate block's type bits 11 are invalid: no stream starts here
        if (installer[i] >> 1) & 3 == 3 {
            i += 1;
            continue;
        }
        let mut z = flate2::Decompress::new(false);
        let r = z.decompress(&installer[i..], &mut out, flate2::FlushDecompress::Finish);
        if matches!(r, Ok(flate2::Status::StreamEnd)) {
            let n = z.total_out() as usize;
            let sha = if p.files.iter().any(|f| f.size as usize == n) { Some(sha256_hex(&out[..n])) } else { None };
            if let Some(k) = (0..p.files.len()).find(|&k| found[k].is_none() && p.files[k].size as usize == n && sha.as_deref() == Some(p.files[k].sha256)) {
                found[k] = Some(out[..n].to_vec());
                i += (z.total_in() as usize).max(1);
                continue;
            }
        }
        i += 1;
    }
    let missing: Vec<&str> = p.files.iter().zip(&found).filter(|(_, f)| f.is_none()).map(|(f, _)| f.path).collect();
    if !missing.is_empty() {
        return Err(format!("{}: not found in the installer: {}", p.name, missing.join(", ")));
    }
    Ok(p.files.iter().zip(found).map(|(f, b)| (f.path, b.unwrap())).collect())
}
