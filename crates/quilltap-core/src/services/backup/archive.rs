//! The archive itself — v4 `createBackup`'s final step (`backup-service.ts:741`).
//!
//! **Mechanism divergence (pre-approved, recorded).** v4 shells out to
//! `zip -r <zipPath> <folderName>` from the temp directory. v5 links the `zip`
//! crate and writes the same logical archive: one deflated entry per staged
//! file, each named `<folderName>/<relative path>`, so `unzip` (and v4's own
//! `parseBackupZip`, which looks for a single `quilltap-backup-*` root) sees an
//! identical tree. The zip FRAMING differs by design — extra fields, entry
//! order, compression level — which is why the differential diffs the extracted
//! tree and never the archive bytes.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

/// Every regular file under `root`, relative and `/`-joined, sorted so an
/// archive is reproducible from a given tree.
fn walk(root: &Path) -> std::io::Result<Vec<String>> {
    fn rec(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                rec(root, &p, out)?;
            } else if p.is_file() {
                out.push(
                    p.strip_prefix(root)
                        .expect("walked under root")
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    rec(root, root, &mut out)?;
    Ok(out)
}

/// Zip `staging_dir`'s contents into `zip_path`, with every entry prefixed by
/// `root_folder` — the archive `zip -r <zip> <folderName>` produces when run
/// from the folder's parent.
pub fn zip_staging_dir(
    staging_dir: &Path,
    root_folder: &str,
    zip_path: &Path,
) -> std::io::Result<()> {
    let file = std::fs::File::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        // v4's `zip -r` writes plain (non-Zip64) entries where it can, but a
        // full-history backup can exceed 4 GB; large_file lets the writer pick
        // Zip64 per entry rather than failing.
        .large_file(true);

    for rel in walk(staging_dir)? {
        zip.start_file(format!("{root_folder}/{rel}"), options)
            .map_err(std::io::Error::other)?;
        // Streamed a chunk at a time so no single staged file (a big PDF blob,
        // a 300 MB llm-logs.json) is ever fully resident.
        let mut src = std::fs::File::open(staging_dir.join(&rel))?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            zip.write_all(&buf[..n])?;
        }
    }
    zip.finish().map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zips_the_tree_under_the_root_folder() {
        let dir = std::env::temp_dir().join(format!("qt-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let staging = dir.join("quilltap-backup-X");
        std::fs::create_dir_all(staging.join("data")).unwrap();
        std::fs::write(staging.join("manifest.json"), b"{}").unwrap();
        std::fs::write(staging.join("data").join("tags.json"), b"[]\n").unwrap();

        let zip_path = dir.join("out.zip");
        zip_staging_dir(&staging, "quilltap-backup-X", &zip_path).unwrap();

        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).expect("read back");
        let names: Vec<String> = archive.file_names().map(str::to_string).collect();
        assert!(names.contains(&"quilltap-backup-X/manifest.json".to_string()));
        assert!(names.contains(&"quilltap-backup-X/data/tags.json".to_string()));
        let mut s = String::new();
        archive
            .by_name("quilltap-backup-X/data/tags.json")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "[]\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
