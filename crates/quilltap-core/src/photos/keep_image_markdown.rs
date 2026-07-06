//! Kept-image Markdown builder + parser — port of v4 `lib/photos/keep-image-markdown.ts`.
//!
//! `keep_image` writes a Markdown document into `doc_mount_file_links.extractedText`
//! for the new vault link. That Markdown carries:
//!   - YAML frontmatter with the tags, the keeping character/user, and the model
//!     used to generate the image;
//!   - the original generation prompt (and a differing revised prompt) so a
//!     non-image-reading LLM can reason about what the image depicts;
//!   - a scene snapshot taken from the chat's `sceneState` at keep-time;
//!   - a footer attributing the save with the optional caption.
//!
//! The `extractedText` is what the mount-index chunk pipeline embeds, so all of
//! this is searchable through the character's vault.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::doc_edit::markdown_parser::serialize_frontmatter;
use crate::jsstr::{is_js_ws, js_trim, utf16_len, JS_WS_CLASS};
use crate::markdown::{body_after, parse_frontmatter};

const SLUG_MAX_LENGTH: usize = 60;
const SLUG_FALLBACK: &str = "kept";
const PROMPT_SLUG_WORD_COUNT: usize = 6;

/// v4 `MIME_TO_EXTENSION`.
fn mime_to_extension(key: &str) -> Option<&'static str> {
    match key {
        "image/webp" => Some("webp"),
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/avif" => Some("avif"),
        "image/heic" => Some("heic"),
        "image/heif" => Some("heif"),
        _ => None,
    }
}

/// v4 `KeptImageAttributionRole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeptImageAttributionRole {
    Character,
    User,
    System,
}

impl KeptImageAttributionRole {
    fn as_str(self) -> &'static str {
        match self {
            KeptImageAttributionRole::Character => "character",
            KeptImageAttributionRole::User => "user",
            KeptImageAttributionRole::System => "system",
        }
    }
}

/// A scene-state character for the Markdown snapshot (the fields
/// `renderSceneStateAsMarkdown` reads off `SceneStateCharacter`).
#[derive(Debug, Clone, Deserialize)]
pub struct SceneStateCharacter {
    #[serde(rename = "characterName")]
    pub character_name: String,
    pub action: String,
    #[serde(default)]
    pub appearance: Option<String>,
    #[serde(default)]
    pub clothing: Option<String>,
}

/// The parsed `chat.sceneState` snapshot the markdown embeds (v4 `SceneState`).
/// Only the fields `renderSceneStateAsMarkdown` reads are modeled.
#[derive(Debug, Clone, Deserialize)]
pub struct SceneState {
    pub location: String,
    #[serde(default)]
    pub characters: Vec<SceneStateCharacter>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// Input to [`build_kept_image_markdown`] (v4 `BuildKeptImageMarkdownInput`).
pub struct BuildKeptImageMarkdownInput<'a> {
    pub generation_prompt: Option<&'a str>,
    pub generation_revised_prompt: Option<&'a str>,
    pub generation_model: Option<&'a str>,
    pub scene_state: Option<&'a SceneState>,
    /// True when `chat.sceneState` was non-null but failed schema validation.
    pub scene_state_malformed: bool,
    pub linked_by_name: &'a str,
    pub linked_by_id: Option<&'a str>,
    /// `None` defaults to `Character` (v4 `input.linkedByRole ?? 'character'`).
    pub linked_by_role: Option<KeptImageAttributionRole>,
    pub tags: &'a [String],
    pub caption: Option<&'a str>,
    pub kept_at: &'a str,
}

/// Build the full Markdown document stored as the link's `extractedText`
/// (v4 `buildKeptImageMarkdown`).
pub fn build_kept_image_markdown(input: &BuildKeptImageMarkdownInput<'_>) -> String {
    let role = input
        .linked_by_role
        .unwrap_or(KeptImageAttributionRole::Character);

    // v4 builds the frontmatter object in this exact key order; YAML.stringify
    // (= our serialize_frontmatter) preserves insertion order.
    let mut frontmatter: Map<String, Value> = Map::new();
    frontmatter.insert(
        "tags".to_string(),
        Value::Array(
            input
                .tags
                .iter()
                .map(|t| Value::String(t.clone()))
                .collect(),
        ),
    );
    frontmatter.insert(
        "linkedBy".to_string(),
        Value::String(input.linked_by_name.to_string()),
    );
    frontmatter.insert(
        "linkedById".to_string(),
        match input.linked_by_id {
            Some(id) => Value::String(id.to_string()),
            None => Value::Null,
        },
    );
    frontmatter.insert(
        "linkedByRole".to_string(),
        Value::String(role.as_str().to_string()),
    );
    frontmatter.insert(
        "generationModel".to_string(),
        match input.generation_model {
            Some(m) => Value::String(m.to_string()),
            None => Value::Null,
        },
    );
    // v4: `---\n${YAML.stringify(frontmatter)}---\n` — exactly serialize_frontmatter.
    let yaml_block = serialize_frontmatter(&frontmatter);

    let mut sections: Vec<String> = Vec::new();

    let prompt = js_trim(input.generation_prompt.unwrap_or("")).to_string();
    if !prompt.is_empty() {
        sections.push(format!("## Original prompt\n\n{prompt}"));
    }

    let revised = js_trim(input.generation_revised_prompt.unwrap_or("")).to_string();
    if !revised.is_empty() && revised != prompt {
        sections.push(format!("## Revised prompt\n\n{revised}"));
    }

    let scene_block =
        render_scene_state_as_markdown(input.scene_state, input.scene_state_malformed);
    if !scene_block.is_empty() {
        sections.push(scene_block);
    }

    sections.push(build_attribution_line(
        input.linked_by_name,
        input.kept_at,
        input.caption,
    ));

    format!("{yaml_block}\n{}\n", sections.join("\n\n"))
}

fn build_attribution_line(linked_by_name: &str, kept_at: &str, caption: Option<&str>) -> String {
    let trimmed_caption = js_trim(caption.unwrap_or(""));
    if !trimmed_caption.is_empty() {
        format!(
            "{linked_by_name} saved this image at {kept_at} with this caption: {trimmed_caption}"
        )
    } else {
        format!("{linked_by_name} saved this image at {kept_at}.")
    }
}

/// Render a `SceneState` snapshot as a small Markdown subsection
/// (v4 `renderSceneStateAsMarkdown`). Empty string when no scene state (so the
/// caller drops the section instead of emitting an empty heading).
pub fn render_scene_state_as_markdown(scene_state: Option<&SceneState>, malformed: bool) -> String {
    if malformed {
        return "### Scene at the time\n\n_scene state unavailable_".to_string();
    }
    let Some(scene) = scene_state else {
        return String::new();
    };

    let mut lines: Vec<String> = vec![format!("### Scene at {}", scene.updated_at), String::new()];
    // v4: `location || '_unspecified_'` — falsy (empty) location → placeholder.
    let location = if scene.location.is_empty() {
        "_unspecified_"
    } else {
        scene.location.as_str()
    };
    lines.push(format!("- **Location**: {location}"));

    if !scene.characters.is_empty() {
        // names = characters.map(c => c.characterName).filter(Boolean).join(', ')
        let names: Vec<&str> = scene
            .characters
            .iter()
            .map(|c| c.character_name.as_str())
            .filter(|n| !n.is_empty())
            .collect();
        if !names.is_empty() {
            lines.push(format!("- **Characters present**: {}", names.join(", ")));
        }
        lines.push(String::new());
        for c in &scene.characters {
            let mut segments: Vec<String> = Vec::new();
            // v4: `if (c.action) segments.push(c.action); else 'no action recorded'`.
            if !c.action.is_empty() {
                segments.push(c.action.clone());
            } else {
                segments.push("no action recorded".to_string());
            }
            if let Some(clothing) = &c.clothing {
                if !clothing.is_empty() {
                    segments.push(format!("wearing {clothing}"));
                }
            }
            if let Some(appearance) = &c.appearance {
                if !appearance.is_empty() {
                    segments.push(appearance.clone());
                }
            }
            lines.push(format!(
                "- **{}** — {}",
                c.character_name,
                segments.join("; ")
            ));
        }
    }

    // v4: `lines.join('\n').trimEnd()`.
    js_trim_end_owned(&lines.join("\n"))
}

/// `String.prototype.trimEnd` over an owned string (JS whitespace set).
fn js_trim_end_owned(s: &str) -> String {
    crate::jsstr::js_trim_end(s).to_string()
}

/// The structured metadata recovered from a link's `extractedText`
/// (v4 `KeptImageFrontmatter`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeptImageFrontmatter {
    pub tags: Vec<String>,
    #[serde(rename = "linkedBy")]
    pub linked_by: Option<String>,
    #[serde(rename = "linkedById")]
    pub linked_by_id: Option<String>,
    /// Role; defaults to `character` for rows written before the field existed.
    #[serde(rename = "linkedByRole")]
    pub linked_by_role: String,
    #[serde(rename = "generationModel")]
    pub generation_model: Option<String>,
    /// The trailing "X saved this image at TS with this caption: …" caption.
    pub caption: Option<String>,
}

impl KeptImageFrontmatter {
    fn empty() -> Self {
        KeptImageFrontmatter {
            tags: Vec::new(),
            linked_by: None,
            linked_by_id: None,
            linked_by_role: "character".to_string(),
            generation_model: None,
            caption: None,
        }
    }
}

/// Reverse of [`build_kept_image_markdown`] — pull the structured metadata back out
/// of a link's `extractedText` (v4 `parseKeptImageFrontmatter`). Used by
/// `list_images` and the chat GET resolver.
pub fn parse_kept_image_frontmatter(extracted_text: Option<&str>) -> KeptImageFrontmatter {
    let Some(text) = extracted_text else {
        return KeptImageFrontmatter::empty();
    };
    if text.is_empty() {
        return KeptImageFrontmatter::empty();
    }

    let parsed = parse_frontmatter(text);
    let data = parsed.data.clone().unwrap_or(Value::Object(Map::new()));

    // tags: Array.isArray(data.tags) ? data.tags.filter(t => typeof t === 'string') : []
    let tags: Vec<String> = data
        .get("tags")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let linked_by = data
        .get("linkedBy")
        .and_then(Value::as_str)
        .map(str::to_string);
    let linked_by_id = data
        .get("linkedById")
        .and_then(Value::as_str)
        .map(str::to_string);
    // roleRaw === 'user' || 'system' || 'character' ? roleRaw : 'character'
    let linked_by_role = match data.get("linkedByRole").and_then(Value::as_str) {
        Some(r @ ("user" | "system" | "character")) => r.to_string(),
        _ => "character".to_string(),
    };
    let generation_model = data
        .get("generationModel")
        .and_then(Value::as_str)
        .map(str::to_string);

    // caption = extractCaptionFromBody(extractedText.slice(bodyStartOffset))
    let body = body_after(text, &parsed);
    let caption = extract_caption_from_body(body);

    KeptImageFrontmatter {
        tags,
        linked_by,
        linked_by_id,
        linked_by_role,
        generation_model,
        caption,
    }
}

/// v4 `CAPTION_REGEX = / saved this image at [^\s]+ with this caption: (.+)$/m`.
/// `[^\s]` is the JS non-whitespace set; `.` excludes JS line terminators; `$` is
/// multiline. The generated markdown uses `\n` line joins only (no `\r`/` `),
/// so the regex-crate `(?m)$` (matches before `\n`) reproduces the JS match on our
/// corpus — the `\r`/`\u{2028}`/`\u{2029}` `$` boundary is a documented minor seam.
static CAPTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?m) saved this image at [^{ws}]+ with this caption: ([^\n\r\u{{2028}}\u{{2029}}]+)$",
        ws = ws_inner()
    ))
    .unwrap()
});

/// The inner chars of [`JS_WS_CLASS`] (drop the leading `[` and trailing `]`) so
/// they can be spliced into a negated class `[^…]`.
fn ws_inner() -> &'static str {
    // JS_WS_CLASS is "[<chars>]"; slice off the brackets.
    &JS_WS_CLASS[1..JS_WS_CLASS.len() - 1]
}

fn extract_caption_from_body(body: &str) -> Option<String> {
    let caps = CAPTION_REGEX.captures(body)?;
    Some(js_trim(caps.get(1)?.as_str()).to_string())
}

/// Build a description block for the Librarian's attach announcement out of a
/// kept-image link's `extractedText` (v4 `buildAttachDescriptionFromKeptImage`):
/// the Markdown body (prompt + scene + attribution) with the YAML frontmatter
/// stripped. `None` when the input has no body content.
pub fn build_attach_description_from_kept_image(extracted_text: Option<&str>) -> Option<String> {
    let text = extracted_text?;
    if text.is_empty() {
        return None;
    }
    let parsed = parse_frontmatter(text);
    let body = js_trim(body_after(text, &parsed));
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

/// Input to [`build_slug_and_filename`] (v4 `BuildSlugAndFilenameInput`).
pub struct BuildSlugAndFilenameInput<'a> {
    pub caption: Option<&'a str>,
    pub generation_prompt: Option<&'a str>,
    pub mime_type: &'a str,
    /// ISO timestamp; colons replaced with hyphens for path safety.
    pub kept_at: &'a str,
}

/// Output of [`build_slug_and_filename`] (v4 `BuildSlugAndFilenameOutput`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildSlugAndFilenameOutput {
    pub slug: String,
    pub filename: String,
    pub extension: String,
}

/// v4 `buildSlugAndFilename`.
pub fn build_slug_and_filename(
    input: &BuildSlugAndFilenameInput<'_>,
) -> BuildSlugAndFilenameOutput {
    // slugSource = (caption?.trim() ? caption : firstWordsOf(generationPrompt ?? '', 6)) ?? ''
    let caption_trimmed_nonempty = input
        .caption
        .map(|c| !js_trim(c).is_empty())
        .unwrap_or(false);
    let slug_source = if caption_trimmed_nonempty {
        input.caption.unwrap_or("").to_string()
    } else {
        first_words_of(
            input.generation_prompt.unwrap_or(""),
            PROMPT_SLUG_WORD_COUNT,
        )
    };

    let slug = {
        let s = slugify(&slug_source);
        if s.is_empty() {
            SLUG_FALLBACK.to_string()
        } else {
            s
        }
    };
    let extension = extension_for_mime(input.mime_type);
    // safeTimestamp = keptAt.replace(/:/g, '-')
    let safe_timestamp = input.kept_at.replace(':', "-");
    let filename = format!("{safe_timestamp}-{slug}.{extension}");
    BuildSlugAndFilenameOutput {
        slug,
        filename,
        extension,
    }
}

/// v4 `slugify`: `.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
/// .slice(0, 60).replace(/-+$/g, '')`. Operates on JS-`toLowerCase`d text; after the
/// non-`[a-z0-9]` collapse the string is ASCII, so the `slice(0,60)` UTF-16 cap == a
/// byte slice.
fn slugify(text: &str) -> String {
    let lowered = text.to_lowercase();
    // replace(/[^a-z0-9]+/g, '-'): collapse runs of non-[a-z0-9] to a single '-'.
    let mut collapsed = String::with_capacity(lowered.len());
    let mut in_run = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            collapsed.push(ch);
            in_run = false;
        } else if !in_run {
            collapsed.push('-');
            in_run = true;
        }
    }
    // replace(/^-+|-+$/g, ''): trim leading/trailing '-'.
    let trimmed = collapsed.trim_matches('-');
    // slice(0, 60) — ASCII by now, so byte==UTF-16.
    let sliced: String = trimmed.chars().take(SLUG_MAX_LENGTH).collect();
    // replace(/-+$/g, ''): drop any trailing '-' revealed by the slice.
    sliced.trim_end_matches('-').to_string()
}

/// v4 `firstWordsOf`: `trimmed.split(/\s+/).slice(0, wordCount).join(' ')`.
fn first_words_of(text: &str, word_count: usize) -> String {
    let trimmed = js_trim(text);
    if trimmed.is_empty() {
        return String::new();
    }
    // JS `split(/\s+/)` on an already-trimmed string yields the non-empty words.
    let words: Vec<&str> = trimmed.split(is_js_ws).filter(|w| !w.is_empty()).collect();
    words
        .into_iter()
        .take(word_count)
        .collect::<Vec<_>>()
        .join(" ")
}

/// v4 `extensionForMime`.
fn extension_for_mime(mime: &str) -> String {
    let key = mime.to_lowercase();
    if let Some(ext) = mime_to_extension(&key) {
        return ext.to_string();
    }
    if let Some(sub) = key.strip_prefix("image/") {
        // sub = key.slice('image/'.length).split(/[+;]/)[0]
        let sub0 = sub.split(['+', ';']).next().unwrap_or("");
        if !sub0.is_empty() {
            return sub0.to_string();
        }
    }
    "bin".to_string()
}

/// Compute the SHA-256 of a UTF-8 string as lowercase hex (v4 `sha256OfString`).
pub fn sha256_of_string(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Compute the SHA-256 of a byte buffer as lowercase hex (v4 `sha256OfBuffer`).
pub fn sha256_of_buffer(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// POSIX-basename a relative path (v4 `basenameOfRelativePath` = `path.posix.basename`).
pub fn basename_of_relative_path(relative_path: &str) -> String {
    // path.posix.basename strips one trailing '/', then returns the last segment.
    let trimmed = if relative_path.len() > 1 {
        relative_path.strip_suffix('/').unwrap_or(relative_path)
    } else {
        relative_path
    };
    match trimmed.rfind('/') {
        None => trimmed.to_string(),
        Some(idx) => trimmed[idx + 1..].to_string(),
    }
}

// UTF-16 length is used by the caller (save_image_to_album) for plainTextLength.
pub(crate) fn extracted_text_utf16_len(text: &str) -> i64 {
    utf16_len(text) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_filename_basic() {
        let out = build_slug_and_filename(&BuildSlugAndFilenameInput {
            caption: Some("A Sunny Day!"),
            generation_prompt: Some("something else entirely here"),
            mime_type: "image/webp",
            kept_at: "2026-05-14T07:22:33.000Z",
        });
        assert_eq!(out.slug, "a-sunny-day");
        assert_eq!(out.extension, "webp");
        assert_eq!(out.filename, "2026-05-14T07-22-33.000Z-a-sunny-day.webp");
    }

    #[test]
    fn slug_falls_back_to_prompt_words_then_kept() {
        let out = build_slug_and_filename(&BuildSlugAndFilenameInput {
            caption: None,
            generation_prompt: Some("one two three four five six seven eight"),
            mime_type: "image/png",
            kept_at: "2026-05-14T07:22:33.000Z",
        });
        assert_eq!(out.slug, "one-two-three-four-five-six");
        assert_eq!(out.extension, "png");

        let empty = build_slug_and_filename(&BuildSlugAndFilenameInput {
            caption: Some("   "),
            generation_prompt: Some("!!!"),
            mime_type: "image/jpeg",
            kept_at: "2026-05-14T07:22:33.000Z",
        });
        assert_eq!(empty.slug, "kept");
        assert_eq!(empty.extension, "jpg");
    }

    #[test]
    fn mime_extension_fallbacks() {
        assert_eq!(extension_for_mime("image/svg+xml"), "svg");
        assert_eq!(extension_for_mime("image/tiff"), "tiff");
        assert_eq!(extension_for_mime("application/octet-stream"), "bin");
    }

    #[test]
    fn build_and_parse_roundtrip_caption_tags() {
        let tags = vec!["sunset".to_string(), "beach".to_string()];
        let md = build_kept_image_markdown(&BuildKeptImageMarkdownInput {
            generation_prompt: Some("a golden sunset over the sea"),
            generation_revised_prompt: Some("a golden sunset over the sea"),
            generation_model: Some("dall-e-3"),
            scene_state: None,
            scene_state_malformed: false,
            linked_by_name: "Aurora",
            linked_by_id: Some("char-1"),
            linked_by_role: None,
            tags: &tags,
            caption: Some("my favourite"),
            kept_at: "2026-05-14T07:22:33.000Z",
        });
        // Frontmatter present; revised == prompt so only "Original prompt".
        assert!(md.starts_with("---\n"));
        assert!(md.contains("## Original prompt\n\na golden sunset over the sea"));
        assert!(!md.contains("## Revised prompt"));
        assert!(md.contains(
            "Aurora saved this image at 2026-05-14T07:22:33.000Z with this caption: my favourite"
        ));

        let meta = parse_kept_image_frontmatter(Some(&md));
        assert_eq!(meta.tags, tags);
        assert_eq!(meta.linked_by.as_deref(), Some("Aurora"));
        assert_eq!(meta.linked_by_id.as_deref(), Some("char-1"));
        assert_eq!(meta.linked_by_role, "character");
        assert_eq!(meta.generation_model.as_deref(), Some("dall-e-3"));
        assert_eq!(meta.caption.as_deref(), Some("my favourite"));
    }

    #[test]
    fn linked_by_role_backcompat_defaults_character() {
        // A row without linkedByRole parses as 'character'.
        let md = "---\nlinkedBy: Bob\n---\n\nBob saved this image at TS.\n";
        let meta = parse_kept_image_frontmatter(Some(md));
        assert_eq!(meta.linked_by_role, "character");
        assert_eq!(meta.linked_by.as_deref(), Some("Bob"));
        assert_eq!(meta.caption, None);
    }

    #[test]
    fn scene_state_render() {
        let scene = SceneState {
            location: "the observatory".to_string(),
            characters: vec![
                SceneStateCharacter {
                    character_name: "Aurora".to_string(),
                    action: "gazing at the stars".to_string(),
                    appearance: None,
                    clothing: Some("a velvet coat".to_string()),
                },
                SceneStateCharacter {
                    character_name: "Bob".to_string(),
                    action: String::new(),
                    appearance: Some("weary".to_string()),
                    clothing: None,
                },
            ],
            updated_at: "2026-05-14T07:00:00.000Z".to_string(),
        };
        let out = render_scene_state_as_markdown(Some(&scene), false);
        assert!(out.starts_with("### Scene at 2026-05-14T07:00:00.000Z\n\n"));
        assert!(out.contains("- **Location**: the observatory"));
        assert!(out.contains("- **Characters present**: Aurora, Bob"));
        assert!(out.contains("- **Aurora** — gazing at the stars; wearing a velvet coat"));
        assert!(out.contains("- **Bob** — no action recorded; weary"));
    }

    #[test]
    fn scene_state_malformed_placeholder() {
        assert_eq!(
            render_scene_state_as_markdown(None, true),
            "### Scene at the time\n\n_scene state unavailable_"
        );
        assert_eq!(render_scene_state_as_markdown(None, false), "");
    }
}
