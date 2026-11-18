//! A minimal, deterministic ZIP writer for submission archives.
//!
//! Entries are stored uncompressed: figures are already compressed, sources
//! are small, and every journal system and `unzip` reads stored entries.
//! Timestamps are fixed so the same export yields the same archive.
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(super) fn write(archive: &Path, entries: &[(String, Vec<u8>)]) -> io::Result<()> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = small(out.len())?;
        let (size, crc) = (small(data.len())?, crc32(data));
        let name_len = u16::try_from(name.len()).map_err(|_| io::Error::other("Name too long"))?;
        // Version 2.0; bit 11: UTF-8 names; method 0 (stored); 1980-01-01 00:00.
        let common = |buf: &mut Vec<u8>| {
            for v in [20u16, 0x0800, 0, 0, 0x0021] {
                buf.extend(v.to_le_bytes());
            }
            for v in [crc, size, size] {
                buf.extend(v.to_le_bytes());
            }
            buf.extend(name_len.to_le_bytes());
            buf.extend(0u16.to_le_bytes());
        };
        out.extend(0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend(name.as_bytes());
        out.extend(data);
        central.extend(0x0201_4b50u32.to_le_bytes());
        // Made by Unix (3), so the external attributes carry rw-r--r--.
        central.extend(0x0314u16.to_le_bytes());
        common(&mut central);
        for v in [0u16, 0, 0] {
            central.extend(v.to_le_bytes());
        }
        central.extend((0o100644u32 << 16).to_le_bytes());
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let count = u16::try_from(entries.len()).map_err(|_| io::Error::other("Too many files"))?;
    let (start, length) = (small(out.len())?, small(central.len())?);
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    for v in [0u16, 0, count, count] {
        out.extend(v.to_le_bytes());
    }
    out.extend(length.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    fs::File::create_new(archive)?.write_all(&out)
}
fn small(n: usize) -> io::Result<u32> {
    u32::try_from(n).map_err(|_| io::Error::other("Submission archive exceeds 4 GB"))
}
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *entry = c;
    }
    !data.iter().fold(!0u32, |c, &b| {
        table[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
    #[test]
    fn archive_round_trips_through_unzip() {
        let dir = std::env::temp_dir().join(format!("folio-zip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("a.zip");
        write(
            &archive,
            &[
                ("main.tex".into(), b"\\documentclass{article}\n".to_vec()),
                ("résumé.bbl".into(), vec![]),
            ],
        )
        .unwrap();
        assert!(write(&archive, &[]).is_err(), "never overwrite");
        // `unzip` is optional on the test machine; the format is checked when present.
        if let Ok(out) = std::process::Command::new("unzip")
            .args(["-t", "-q"])
            .arg(&archive)
            .output()
        {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
