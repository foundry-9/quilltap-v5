//! The committed sample-content seed assets, embedded into the binary (v4 reads
//! them from `first-startup/` at `process.cwd()`; a native binary carries them so
//! there is no runtime filesystem dependency). Byte-identical to v4's copies at
//! `a7b1398d` — see the unit's status-log entry for their sha256.

/// The legacy monolithic-JSON sample-content export
/// (`first-startup/imports/lorian-and-riya.qtap`): 2 characters (Lorian, Riya),
/// 4 wardrobe items each, 42 memories. The ONLY `.qtap` the seed path loads.
pub const LORIAN_AND_RIYA_QTAP: &str =
    include_str!("../../../../../assets/first-startup/imports/lorian-and-riya.qtap");

/// One seed avatar: a character name (from the file basename) + its image bytes +
/// mime type. v4 `getSeedAvatars` maps `first-startup/avatars/*.{webp,png,jpg,jpeg}`
/// to characters by case-insensitive basename.
pub struct SeedAvatar {
    pub character_name: &'static str,
    pub filename: &'static str,
    pub content: &'static [u8],
    pub mime_type: &'static str,
}

/// Lorian's avatar (`first-startup/avatars/Lorian.webp`). Riya has no avatar file
/// (matching v4). Already-WebP, so the vault write stores it as-is (no transcode).
pub const LORIAN_WEBP: &[u8] =
    include_bytes!("../../../../../assets/first-startup/avatars/Lorian.webp");

/// The seed avatars, sorted by filename (v4 `getSeedAvatars` sorts the dir).
pub const SEED_AVATARS: &[SeedAvatar] = &[SeedAvatar {
    character_name: "Lorian",
    filename: "Lorian.webp",
    content: LORIAN_WEBP,
    mime_type: "image/webp",
}];
