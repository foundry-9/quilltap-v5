//! The custom-tool definition format, as data the engine itself serves.
//!
//! `edit_custom_tool`'s `action: "schema"` hands the model v4's published JSON
//! Schema and two worked specimens (v4 imports the three files at runtime).
//! A Rust engine has no `public/` or `docs/` tree to read from, so the three
//! are vendored here as BYTE-copies of v4 at `322a5488e`
//! (`4.11.0-dev.53`) and `include_str!`'d — the `generators/qtap_schema.rs`
//! arrangement. They live under `pascal/specimens/` rather than reaching into
//! `apps/web/public/` or the `docs/v4/` mirror, so the mirror stays a pure
//! mirror (round ruling R-E).
//!
//! | file | bytes | md5 |
//! |---|---|---|
//! | `qtap-custom-tool.schema.json` (v4 `public/schemas/`) | 63,595 | `670ade1d0accb312d58f6c6c034285ae` |
//! | `CUSTOM_TOOL_SPEC.json` (v4 `docs/developer/`) | 12,660 | `9e7954b83f3f6e3104b9d8ca814ede68` |
//! | `CUSTOM_TOOL_SPEC_DICE.json` (v4 `docs/developer/`) | 1,951 | `56a7f0d45ca8883418bd6e961bb29775` |
//!
//! Re-vendoring obligation: `custom_tool_schema_embed_guard` fails the moment
//! any copy differs from the SPA's `apps/web/public/schemas/` copy, the
//! `docs/v4/` mirrors, or a v4 checkout — all of them must move together.

/// v4's `public/schemas/qtap-custom-tool.schema.json`.
pub const QTAP_CUSTOM_TOOL_SCHEMA_JSON: &str =
    include_str!("specimens/qtap-custom-tool.schema.json");

/// v4's `docs/developer/CUSTOM_TOOL_SPEC.json` — the range specimen.
pub const CUSTOM_TOOL_SPEC_JSON: &str = include_str!("specimens/CUSTOM_TOOL_SPEC.json");

/// v4's `docs/developer/CUSTOM_TOOL_SPEC_DICE.json` — the dice specimen.
pub const CUSTOM_TOOL_SPEC_DICE_JSON: &str = include_str!("specimens/CUSTOM_TOOL_SPEC_DICE.json");
