//! Photo album path helpers — port of v4 `lib/photos/photos-paths.ts`.
//!
//! A character's photo album is a `photos/` subfolder inside their character
//! vault, not a separate mount point. Centralising the folder name keeps the LLM
//! tools, the chat GET resolver, and any future migration in lockstep.

/// The photo-album folder name inside a vault.
pub const PHOTOS_FOLDER: &str = "photos";

/// Compose a relative path inside a vault's `photos/` folder (v4
/// `buildPhotosRelativePath`).
pub fn build_photos_relative_path(filename: &str) -> String {
    format!("{PHOTOS_FOLDER}/{filename}")
}

/// True when a `doc_mount_file_links.relativePath` lives in a `photos/` folder
/// (v4 `isPhotosRelativePath`). Case-insensitive, matching the rest of the
/// mount-index lookups: the POSIX dirname is lower-cased and compared to
/// `photos` or a `photos/…` prefix.
///
/// The ONE home since P4.91 — v4 has one too (`lib/photos/photos-paths.ts`,
/// imported by all 21 of its call sites), and v5 had — from Phase 3 (W4.8)
/// until P4.91 — a second copy in `db::doc_mount_file_links` that differed on
/// trailing-slash runs. Driven by
/// `photos_relative_path_equivalence` over v4's real function.
pub fn is_photos_relative_path(relative_path: Option<&str>) -> bool {
    let Some(rel) = relative_path else {
        return false;
    };
    if rel.is_empty() {
        return false;
    }
    let folder = posix_dirname(rel).to_lowercase();
    folder == PHOTOS_FOLDER || folder.starts_with(&format!("{PHOTOS_FOLDER}/"))
}

/// Where `migrate-character-avatars-to-vaults-v1` parked a pre-existing portrait
/// (v4 `LEGACY_MAIN_AVATAR_PATH`).
pub const LEGACY_MAIN_AVATAR_PATH: &str = "images/avatar.webp";

/// True when a vault link is part of a character's **photo album** — what the
/// Aurora gallery tab shows and what the character-detail `photos` figure counts
/// (v4 `isCharacterAlbumRelativePath`, `4dcbe0d21`).
///
/// One predicate, two readers ([`crate::photos::character_gallery_service::
/// list_character_gallery`] and the `?action=stats` figure in
/// `api::characters::character_stats`), because a second copy is how the grid
/// and the count come to disagree — which is exactly what v4 had, and what this
/// replaces: both sites previously hand-rolled
/// `is_photos_relative_path || "images/avatar.webp" || starts_with("images/history/")`.
///
/// `images/history/` is deliberately excluded: those are avatar rolls — the
/// configuration cache's working stock — and they have their own section, fed by
/// [`crate::photos::avatar_rolls_service`]. The canonical `images/avatar.webp`
/// portrait *is* album material, so a character who has never kept anything
/// still has a face on the page.
pub fn is_character_album_relative_path(relative_path: Option<&str>) -> bool {
    let Some(rel) = relative_path.filter(|s| !s.is_empty()) else {
        return false;
    };
    if is_photos_relative_path(Some(rel)) {
        return true;
    }
    rel.to_lowercase() == LEGACY_MAIN_AVATAR_PATH
}

/// JS `path.posix.dirname(p)` — Node's own algorithm, moved here from the
/// second copy this module used to share the predicate with
/// (`db::doc_mount_file_links`, deleted at P4.91).
///
/// Empty → `"."`. Otherwise: walk backwards from the end, SKIPPING a run of
/// trailing slashes, and answer the substring before the last separator found
/// (`"/"` or `"//"` for the rooted degenerate shapes, `"."` when no separator
/// survives). Skipping the whole run is the part a one-slash `strip_suffix`
/// gets wrong: `"photos//"` is `"."` here and `"photos"` there, which is the
/// disagreement `photos_relative_path_equivalence` now pins (measured on Node
/// 24.13.1 — three of its thirty rows red before this landed).
///
/// The `has_root && end == 1` arm answers `"//"`, not `"/"` — Node's
/// `dirname("//photos")` is `"//"`. The copy this was moved from answered
/// `"/"` there; the predicate cannot tell the two apart (neither is `photos`
/// nor starts with `photos/`), so the corpus is blind to it and
/// [`tests::posix_dirname_matches_node`] pins it directly instead.
fn posix_dirname(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let bytes = p.as_bytes();
    let has_root = bytes[0] == b'/';
    // The index of the last slash that is not part of the trailing run.
    let mut end: Option<usize> = None;
    let mut matched_slash = true;
    let mut i = p.len();
    while i > 1 {
        i -= 1;
        if bytes[i] == b'/' {
            if !matched_slash {
                end = Some(i);
                break;
            }
        } else {
            // The first non-separator seen from the right: everything after it
            // is no longer a trailing run.
            matched_slash = false;
        }
    }
    match end {
        None if has_root => "/".to_string(),
        None => ".".to_string(),
        Some(1) if has_root => "//".to_string(),
        Some(e) => p[..e].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_relative_path_detection() {
        assert!(is_photos_relative_path(Some("photos/a.webp")));
        assert!(is_photos_relative_path(Some("Photos/a.webp"))); // case-insensitive
        assert!(is_photos_relative_path(Some("PHOTOS/a.webp")));
        assert!(is_photos_relative_path(Some("photos/sub/a.webp")));
        assert!(!is_photos_relative_path(Some("a.webp"))); // dirname "."
        assert!(!is_photos_relative_path(Some("other/a.webp")));
        assert!(!is_photos_relative_path(Some("images/a.webp")));
        assert!(!is_photos_relative_path(Some("my-photos/a.webp"))); // dirname "my-photos"
        assert!(!is_photos_relative_path(Some("photosx/a.webp")));
        assert!(!is_photos_relative_path(None));
        assert!(!is_photos_relative_path(Some("")));

        // The trailing-slash-run shapes (P4.91). Node skips the WHOLE run, so
        // the dirname is "." and these are NOT album paths — the three rows
        // `photos_relative_path_equivalence` reddened on before the Node loop
        // moved in here. (The consolidated half of what
        // `services::maintenance`'s near-duplicate suite used to assert.)
        assert!(!is_photos_relative_path(Some("photos//")));
        assert!(!is_photos_relative_path(Some("photos///")));
        assert!(!is_photos_relative_path(Some("PHOTOS//")));
        assert!(!is_photos_relative_path(Some("a//")));
        // The control: ONE trailing slash is stripped, by Node and by the
        // one-slash spelling this replaced alike.
        assert!(is_photos_relative_path(Some("photos/a.webp/")));
        // An INTERIOR run leaves the dirname trailing a "/", which is the one
        // shape the run rule makes TRUE through the `photos/` prefix test.
        assert!(is_photos_relative_path(Some("photos//a.webp")));
    }

    /// `posix_dirname` against `path.posix.dirname` on Node 24.13.1, output
    /// captured verbatim. The differential drives the PREDICATE, which folds
    /// most of these to the same answer — the two root arms in particular are
    /// invisible to it (`"/"` and `"//"` are both non-album) — so the helper is
    /// pinned here directly.
    #[test]
    fn posix_dirname_matches_node() {
        for (input, want) in [
            ("photos/a.webp", "photos"),
            ("Photos/a.webp", "Photos"),
            ("photos/sub/a.webp", "photos/sub"),
            ("photos/2026/01/a.webp", "photos/2026/01"),
            ("a.webp", "."),
            ("/photos/a.webp", "/photos"),
            ("a/photos/x.webp", "a/photos"),
            ("", "."),
            // Trailing-slash runs: skipped whole.
            ("photos/a.webp/", "photos"),
            ("photos//", "."),
            ("photos///", "."),
            ("a//", "."),
            ("photos/sub//", "photos"),
            ("/photos//", "/"),
            // Interior run: the separator before it wins, trailing "/" and all.
            ("photos//a.webp", "photos/"),
            // Folder-only and degenerate.
            ("photos", "."),
            ("photos/", "."),
            ("/", "/"),
            ("//", "/"),
            (".", "."),
            ("./photos/a.webp", "./photos"),
            // The `has_root && end == 1` arm: "//", not "/".
            ("//photos", "//"),
            ("//photos/a.webp", "//photos"),
            ("///photos", "//"),
        ] {
            assert_eq!(
                posix_dirname(input),
                want,
                "posix_dirname({input:?}) should be Node's {want:?}"
            );
        }
    }

    #[test]
    fn character_album_membership() {
        // The `photos/` arm delegates (every `is_photos_relative_path` case).
        assert!(is_character_album_relative_path(Some("photos/a.webp")));
        assert!(is_character_album_relative_path(Some("Photos/sub/a.webp")));
        // The canonical portrait, case-insensitively (v4 lower-cases the whole
        // path and compares).
        assert!(is_character_album_relative_path(Some("images/avatar.webp")));
        assert!(is_character_album_relative_path(Some("Images/Avatar.WEBP")));
        // The one behaviour change of v4 `4dcbe0d21`: an avatar roll is NOT
        // album material any more.
        assert!(!is_character_album_relative_path(Some(
            "images/history/roll-01.webp"
        )));
        assert!(!is_character_album_relative_path(Some(
            "IMAGES/HISTORY/roll-01.webp"
        )));
        // Neighbours that must not be swept in by a loose prefix test.
        assert!(!is_character_album_relative_path(Some("images/other.webp")));
        assert!(!is_character_album_relative_path(Some(
            "images/avatar.webp.bak"
        )));
        assert!(!is_character_album_relative_path(None));
        assert!(!is_character_album_relative_path(Some("")));
    }

    #[test]
    fn build_photos_relative_path_prefixes() {
        assert_eq!(build_photos_relative_path("x.png"), "photos/x.png");
    }
}
