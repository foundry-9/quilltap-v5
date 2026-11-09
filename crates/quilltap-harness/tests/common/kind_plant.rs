//! P4.D371 — the in-case vault plant for the generator families' assistant-kind
//! rows (U1). Pulled in by `#[path = "common/kind_plant.rs"] mod kind_plant;`
//! from each family that needs it, so the shared `common/mod.rs` stays
//! untouched.
//!
//! The plant writes the given bytes to the character's vault `properties.json`
//! through the REAL `write_database_document` (the twin of the v4 oracle's
//! `writeDatabaseDocument` — the content-addressed document store takes a
//! proper insert, never a raw row), on a fresh COPY of the committed pair,
//! before the family opens it. It self-checks that the planted bytes are what
//! the vault link now reads; whether the READ PATH then carries the kind is
//! exactly what the planted rows measure (red on the P4.D371 branch by design
//! until P4.D369's read overlay lands — H1).

#![allow(dead_code)]

use std::path::Path;

use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::Writer;

/// Write `properties` to `character_id`'s vault `properties.json` on the
/// `main` / `mount` copies (keyed by `pepper`), then read it back.
pub fn plant_vault_properties(
    main: &Path,
    mount: &Path,
    pepper: &str,
    character_id: &str,
    properties: &str,
) {
    let main_w = Writer::open_writable(main, pepper).expect("open the main copy for the kind plant");
    let mount_point_id: String = main_w
        .connection()
        .query_row(
            "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
            [character_id],
            |row| row.get(0),
        )
        .unwrap_or_else(|e| panic!("kind plant: no vault FK for {character_id}: {e}"));
    drop(main_w);
    let mount_w =
        Writer::open_writable(mount, pepper).expect("open the mount copy for the kind plant");
    DocMountFileLinksRepository::new(mount_w.connection())
        .write_database_document(&mount_point_id, "properties.json", properties)
        .unwrap_or_else(|e| panic!("kind plant: write properties.json: {e}"));
    let read_back: String = mount_w
        .connection()
        .query_row(
            "SELECT d.content FROM doc_mount_file_links l \
              JOIN doc_mount_documents d ON d.fileId = l.fileId \
             WHERE l.mountPointId = ?1 AND l.relativePath = 'properties.json'",
            [&mount_point_id],
            |row| row.get(0),
        )
        .expect("kind plant: read properties.json back");
    assert_eq!(read_back, properties, "kind plant: the vault link reads other bytes");
}
