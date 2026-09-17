//! The image-profile options schema the host renders for OpenAI (v4
//! `d8d2890ee`, `plugins/dist/qtap-plugin-openai/image-options-schema.ts`).
//!
//! Built per model from [`crate::model::openai_image_models`]: the quality list
//! is the selected family's own tiers, the size list its own resolutions, and
//! the GPT Image extras appear only for the families that accept them. The
//! editor refetches this whenever the selected model changes, so it never
//! offers a knob the chosen model would reject.
//!
//! Storage keys are the OpenAI wire names, because that is what the OPENAI
//! dialect reads back off `ImageGenParams.profile_parameters` — except `size`,
//! `quality` and `style`, which the host lifts onto the named
//! [`ImageGenParams`](crate::model::image::ImageGenParams) fields and the
//! dialect therefore reads from `params`.
//!
//! Every string here is byte-copied from v4's file (the standing
//! `byte-exact-static-data-transcription` rule): note the U+2019 apostrophes in
//! the `max`-capable quality help text and in `Auto — OpenAI’s standard
//! filtering`, against the ASCII one in `OpenAI's own content filter`, and the
//! U+00D7 `×` in the size labels.

use serde_json::{json, Map, Value};

use super::openai_image_models::{
    arbitrary_size_rules, find_image_model, js_num, parse_size, OpenAIImageModel,
    OPENAI_IMAGE_MODELS,
};

/// Leading choice on every optional enum: store nothing, let the model decide
/// (v4 `MODEL_DEFAULT`).
fn model_default() -> Value {
    json!({ "value": "", "label": "(model default)" })
}

/// v4 `QUALITY_LABELS`.
fn quality_label(q: &str) -> &'static str {
    match q {
        "auto" => "Auto — the model chooses",
        "low" => "Low — fastest and cheapest",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Extra High",
        "max" => "Max — the finest the model offers",
        "standard" => "Standard",
        "hd" => "HD — finer detail",
        // v4's record is total over `OpenAIImageQuality`; the table can hold no
        // other tier, so this arm is unreachable by construction.
        _ => "",
    }
}

/// v4 `QUALITY_DESCRIPTIONS` — a Partial record, so only two tiers carry one.
fn quality_description(q: &str) -> Option<&'static str> {
    match q {
        "xhigh" => Some("GPT Image 2.5 only. Slower and dearer than High."),
        "max" => Some("GPT Image 2.5 only. The slowest and most expensive tier."),
        _ => None,
    }
}

/// Describe a concrete size for the picker: shape first, then the pixels (v4
/// `sizeLabel`).
fn size_label(size: &str) -> String {
    if size == "auto" {
        return "Auto — the model chooses the shape".to_string();
    }
    let Some(parsed) = parse_size(size) else {
        return size.to_string();
    };
    let (width, height) = (parsed.width, parsed.height);
    let shape = if width == height {
        "Square"
    } else if width > height {
        "Landscape"
    } else {
        "Portrait"
    };
    format!("{shape} ({}×{})", js_num(width), js_num(height))
}

/// Flag the resolutions OpenAI's own docs still call experimental (v4
/// `sizeDescription`).
fn size_description(size: &str) -> Option<&'static str> {
    let parsed = parse_size(size)?;
    if parsed.width * parsed.height > arbitrary_size_rules::EXPERIMENTAL_ABOVE_PIXELS {
        Some("Experimental resolution — slower, and the model may decline it.")
    } else {
        None
    }
}

/// An enum value object: `{value, label}` plus `description` only when present
/// (v4's `...(x ? { description: x } : {})` spread).
fn enum_value(value: &str, label: &str, description: Option<&str>) -> Value {
    let mut o = Map::new();
    o.insert("value".into(), Value::String(value.to_string()));
    o.insert("label".into(), Value::String(label.to_string()));
    if let Some(d) = description {
        o.insert("description".into(), Value::String(d.to_string()));
    }
    Value::Object(o)
}

/// v4 `qualityField`.
fn quality_field(caps: &OpenAIImageModel) -> Value {
    let help = if caps.qualities.contains(&"max") {
        "How much effort the model spends on the image. Extra High and Max are GPT Image 2.5’s premium tiers — sharper detail, at a higher price and a longer wait."
    } else {
        "How much effort the model spends on the image. Higher tiers cost more and take longer."
    };
    let mut values = vec![model_default()];
    values.extend(
        caps.qualities
            .iter()
            .map(|q| enum_value(q, quality_label(q), quality_description(q))),
    );
    json!({
        "key": "quality",
        "label": "Quality",
        "type": "enum",
        "default": "",
        "helpText": help,
        "enumValues": values,
    })
}

/// v4 `sizeField`.
fn size_field(caps: &OpenAIImageModel) -> Value {
    let help = if caps.arbitrary_sizes {
        format!(
            "Default dimensions for this profile. This model also accepts any size whose edges divide by {}, at an aspect ratio between 1:{} and {}:1, up to {}×2160 — the list below is a selection, not the limit. Asking for a portrait or landscape image in chat overrides this.",
            js_num(arbitrary_size_rules::EDGE_MULTIPLE),
            js_num(arbitrary_size_rules::MAX_ASPECT),
            js_num(arbitrary_size_rules::MAX_ASPECT),
            js_num(arbitrary_size_rules::MAX_EDGE),
        )
    } else {
        "Default dimensions for this profile. Asking for a portrait or landscape image in chat overrides this.".to_string()
    };
    let mut values = vec![model_default()];
    values.extend(
        caps.sizes
            .iter()
            .map(|s| enum_value(s, &size_label(s), size_description(s))),
    );
    json!({
        "key": "size",
        "label": "Default Size",
        "type": "enum",
        "default": "",
        "helpText": help,
        "enumValues": values,
    })
}

/// v4 `STYLE_FIELD`.
fn style_field() -> Value {
    json!({
        "key": "style",
        "label": "Style",
        "type": "enum",
        "default": "",
        "helpText": "DALL·E 3 only. Vivid leans dramatic and hyper-real; Natural is more restrained.",
        "enumValues": [
            model_default(),
            { "value": "vivid", "label": "Vivid — dramatic, hyper-real" },
            { "value": "natural", "label": "Natural — realistic, less exaggerated" },
        ],
    })
}

/// v4 `BACKGROUND_FIELD`.
fn background_field() -> Value {
    json!({
        "key": "background",
        "label": "Background",
        "type": "enum",
        "default": "",
        "helpText": "Transparent backgrounds need a PNG or WebP output format; ask for one with JPEG selected and the format is switched to PNG so the transparency survives.",
        "enumValues": [
            model_default(),
            { "value": "auto", "label": "Auto — the model chooses" },
            { "value": "opaque", "label": "Opaque — always a filled background" },
            { "value": "transparent", "label": "Transparent — cut-out subject" },
        ],
    })
}

/// v4 `OUTPUT_FORMAT_FIELD`.
fn output_format_field() -> Value {
    json!({
        "key": "output_format",
        "label": "Output Format",
        "type": "enum",
        "default": "",
        "helpText": "The file format the model returns. PNG is the default and the safest for transparency.",
        "enumValues": [
            model_default(),
            { "value": "png", "label": "PNG — lossless, supports transparency" },
            { "value": "webp", "label": "WebP — smaller, supports transparency" },
            { "value": "jpeg", "label": "JPEG — smallest, no transparency" },
        ],
    })
}

/// v4 `OUTPUT_COMPRESSION_FIELD` — `type: 'number'` with NO `min`/`max`/
/// `default`: the field carries only its help text, and the provider's
/// `readIntInRange` is what enforces 0–100.
fn output_compression_field() -> Value {
    json!({
        "key": "output_compression",
        "label": "Output Compression",
        "type": "number",
        "helpText": "Compression level from 0 to 100, applied only when the output format is WebP or JPEG. Higher keeps more detail; the model defaults to 100. Ignored for PNG.",
    })
}

/// v4 `MODERATION_FIELD` — note the ASCII apostrophe in `OpenAI's own content
/// filter` against the U+2019 in `Auto — OpenAI’s standard filtering`; both are
/// v4's.
fn moderation_field() -> Value {
    json!({
        "key": "moderation",
        "label": "Moderation",
        "type": "enum",
        "default": "",
        "helpText": "OpenAI's own content filter for GPT Image models. Low is less restrictive; it does not disable filtering, and OpenAI's usage policies still apply.",
        "enumValues": [
            model_default(),
            { "value": "auto", "label": "Auto — OpenAI’s standard filtering" },
            { "value": "low", "label": "Low — less restrictive" },
        ],
    })
}

/// Build the editor schema for `model_name` (v4
/// `getOpenAIImageOptionsSchema`).
///
/// An unknown or unspecified model gets the widest family's schema — the GPT
/// Image 2.5 one — so the editor is never empty while the model list is still
/// loading or when a profile names a model released after this table shipped.
pub fn openai_image_options_schema(model_name: Option<&str>) -> Value {
    let caps = find_image_model(model_name).unwrap_or(&OPENAI_IMAGE_MODELS[0]);

    let mut fields = vec![quality_field(caps), size_field(caps)];
    if caps.supports_style {
        fields.push(style_field());
    }

    let mut groups = vec![json!({ "title": "Image Parameters", "fields": fields })];

    if caps.gpt_image {
        groups.push(json!({
            "title": "GPT Image Output",
            "helpText": "Parameters the GPT Image families accept. DALL·E models ignore this section because it is not offered for them.",
            "fields": [
                background_field(),
                output_format_field(),
                output_compression_field(),
                moderation_field(),
            ],
        }));
    }

    json!({ "groups": groups })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group_titles(v: &Value) -> Vec<String> {
        v["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["title"].as_str().unwrap().to_string())
            .collect()
    }

    fn field_keys(v: &Value, group: usize) -> Vec<String> {
        v["groups"][group]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["key"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn unknown_and_absent_models_get_the_first_table_entry() {
        let absent = openai_image_options_schema(None);
        let unknown = openai_image_options_schema(Some("gpt-image-3-supernova"));
        let sunburst = openai_image_options_schema(Some("gpt-image-2.5-sunburst"));
        assert_eq!(absent, sunburst);
        assert_eq!(unknown, sunburst);
    }

    #[test]
    fn style_only_for_dall_e_3_and_the_output_group_only_for_gpt_image() {
        let dalle3 = openai_image_options_schema(Some("dall-e-3"));
        assert_eq!(group_titles(&dalle3), vec!["Image Parameters"]);
        assert_eq!(field_keys(&dalle3, 0), vec!["quality", "size", "style"]);

        let dalle2 = openai_image_options_schema(Some("dall-e-2"));
        assert_eq!(group_titles(&dalle2), vec!["Image Parameters"]);
        assert_eq!(field_keys(&dalle2, 0), vec!["quality", "size"]);

        let gpt = openai_image_options_schema(Some("gpt-image-1"));
        assert_eq!(
            group_titles(&gpt),
            vec!["Image Parameters", "GPT Image Output"]
        );
        assert_eq!(field_keys(&gpt, 0), vec!["quality", "size"]);
        assert_eq!(
            field_keys(&gpt, 1),
            vec![
                "background",
                "output_format",
                "output_compression",
                "moderation"
            ]
        );
    }

    /// The two premium tiers carry a `description`; the four ordinary ones do
    /// not, and the `max`-capable help text is the other of the two variants.
    #[test]
    fn premium_tiers_are_described_only_where_they_exist() {
        let sunburst = openai_image_options_schema(Some("gpt-image-2.5-sunburst"));
        let quality = &sunburst["groups"][0]["fields"][0];
        assert!(quality["helpText"]
            .as_str()
            .unwrap()
            .contains("GPT Image 2.5’s premium tiers"));
        let values = quality["enumValues"].as_array().unwrap();
        assert_eq!(values.len(), 7, "(model default) + six tiers");
        assert_eq!(values[0], model_default());
        assert_eq!(values[5]["value"], "xhigh");
        assert_eq!(
            values[5]["description"],
            "GPT Image 2.5 only. Slower and dearer than High."
        );
        assert!(values[1].get("description").is_none(), "auto has none");

        let two = openai_image_options_schema(Some("gpt-image-2"));
        assert_eq!(
            two["groups"][0]["fields"][0]["helpText"],
            "How much effort the model spends on the image. Higher tiers cost more and take longer."
        );
    }

    /// The size picker: v4's shape labels with a U+00D7, `auto`'s own sentence,
    /// and the experimental flag on exactly the two entries above 2560×1440.
    #[test]
    fn size_labels_and_the_experimental_flag() {
        assert_eq!(size_label("auto"), "Auto — the model chooses the shape");
        assert_eq!(size_label("1024x1024"), "Square (1024×1024)");
        assert_eq!(size_label("1536x1024"), "Landscape (1536×1024)");
        assert_eq!(size_label("1024x1536"), "Portrait (1024×1536)");
        // An unparseable entry comes back verbatim, as v4's failed parse does.
        assert_eq!(size_label("wide"), "wide");

        let flare = openai_image_options_schema(Some("gpt-image-2.5-flare"));
        let values = flare["groups"][0]["fields"][1]["enumValues"]
            .as_array()
            .unwrap()
            .clone();
        let described: Vec<&str> = values
            .iter()
            .filter(|v| v.get("description").is_some())
            .map(|v| v["value"].as_str().unwrap())
            .collect();
        // 2048×2048 = 4,194,304 clears the 2560×1440 budget too.
        assert_eq!(described, vec!["2048x2048", "3840x2160", "2160x3840"]);
        // 2560x1440 is exactly the threshold, so it is NOT flagged (`>`).
        assert_eq!(size_description("2560x1440"), None);
        assert!(size_description("3840x2160").is_some());
    }

    /// The arbitrary-size help text renders the rule constants as JS does.
    #[test]
    fn arbitrary_size_help_text_renders_the_constants() {
        let sunburst = openai_image_options_schema(Some("gpt-image-2.5-sunburst"));
        assert_eq!(
            sunburst["groups"][0]["fields"][1]["helpText"],
            "Default dimensions for this profile. This model also accepts any size whose edges divide by 16, at an aspect ratio between 1:3 and 3:1, up to 3840×2160 — the list below is a selection, not the limit. Asking for a portrait or landscape image in chat overrides this."
        );
        let one = openai_image_options_schema(Some("gpt-image-1"));
        assert_eq!(
            one["groups"][0]["fields"][1]["helpText"],
            "Default dimensions for this profile. Asking for a portrait or landscape image in chat overrides this."
        );
    }

    /// `output_compression` is v4's ONE number field, and it carries no
    /// `min`/`max`/`default` — a mutation adding them would change the rendered
    /// editor.
    #[test]
    fn output_compression_carries_only_its_help_text() {
        let f = output_compression_field();
        let keys: Vec<&String> = f.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["key", "label", "type", "helpText"]);
    }
}
