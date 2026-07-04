//! Text-block prompt builder — port of v4 `lib/tools/legacy/text-block-prompt.ts`.
//!
//! Static per-tool doc blocks assembled flag-conditionally. The block bodies are
//! transcribed verbatim from v4 (raw strings preserve the leading newline each
//! template literal carries).

use crate::jsstr::js_trim;

/// Which tools to document. v4 `TextBlockPromptOptions` — every flag is
/// `Option<bool>` because `search` defaults to on (`search !== false`) and the
/// others gate on truthiness.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextBlockPromptOptions {
    pub whisper: Option<bool>,
    pub search: Option<bool>,
    pub image_generation: Option<bool>,
    pub web_search: Option<bool>,
    pub state: Option<bool>,
    pub rng: Option<bool>,
    pub project_info: Option<bool>,
    pub help_search: Option<bool>,
    pub help_settings: Option<bool>,
    pub help_navigate: Option<bool>,
    pub create_note: Option<bool>,
    pub wardrobe_list: Option<bool>,
    pub wardrobe_read: Option<bool>,
    pub wardrobe_wear: Option<bool>,
    pub wardrobe_take_off: Option<bool>,
    pub wardrobe_create: Option<bool>,
    pub wardrobe_update: Option<bool>,
    pub wardrobe_archive: Option<bool>,
}

fn on(v: Option<bool>) -> bool {
    v.unwrap_or(false)
}

const WHISPER: &str = r#"
### Whisper (Private Message)
Send a private message to a specific character that other characters won't see.
Format: [[WHISPER to="character name"]]your private message[[/WHISPER]]
Example: [[WHISPER to="Elena"]]I need to tell you something the others shouldn't hear.[[/WHISPER]]"#;

const SEARCH: &str = r#"
### Search
Search the Scriptorium for information about past conversations, preferences, or facts.
Format: [[SEARCH]]what to search for[[/SEARCH]]
Example: [[SEARCH]]user's favorite food[[/SEARCH]]
Example: [[SEARCH limit="3"]]what we discussed about the garden[[/SEARCH]]"#;

const IMAGE_GENERATION: &str = r#"
### Image Generation
Generate an image to show in the conversation. Describe the image in detail.
Format: [[GENERATE_IMAGE]]detailed description of the image[[/GENERATE_IMAGE]]
Example: [[GENERATE_IMAGE]]a cozy coffee shop on a rainy day, warm lighting, steaming cup of coffee on a wooden table[[/GENERATE_IMAGE]]"#;

const WEB_SEARCH: &str = r#"
### Web Search
Search the web for current information that isn't in your training data.
Format: [[SEARCH_WEB]]search query[[/SEARCH_WEB]]
Example: [[SEARCH_WEB]]latest news about space exploration[[/SEARCH_WEB]]"#;

const STATE: &str = r#"
### State Management
Get or set persistent state values that survive across messages.
Format (get): [[STATE operation="get" key="hp" /]]
Format (set): [[STATE operation="set" key="hp" value="85" /]]
Format (list): [[STATE operation="list" /]]"#;

const RNG: &str = r#"
### Dice / Random Number
Roll dice or generate random numbers.
Format: [[RNG type="d20" /]]
Format: [[RNG type="d6" count="3" /]]
Format: [[RNG type="number" min="1" max="100" /]]"#;

const PROJECT_INFO: &str = r#"
### Project Info
Get information about the current project — name, description, character roster, instructions.
Format: [[PROJECT_INFO action="get_info" /]]
Format: [[PROJECT_INFO action="get_instructions" /]]"#;

const HELP_SEARCH: &str = r#"
### Help Search
Search the help documentation for information about features and capabilities.
Format: [[HELP_SEARCH]]how do I use memories[[/HELP_SEARCH]]"#;

const HELP_SETTINGS: &str = r#"
### Help Settings
Read instance settings to understand and assist with the current configuration. API keys are never shown.
Format: [[HELP_SETTINGS category="overview" /]]
Categories: overview, chat, connections, embeddings, images, appearance, templates, system"#;

const HELP_NAVIGATE: &str = r#"
### Help Navigate
Navigate the user's browser to a specific Quilltap page or settings section.
Format: [[HELP_NAVIGATE url="/settings?tab=chat&section=dangerous-content" /]]"#;

const CREATE_NOTE: &str = r#"
### Create Note
Create a note to remember something for later.
Format: [[CREATE_NOTE title="Meeting Notes"]]content of the note[[/CREATE_NOTE]]"#;

const WARDROBE_LIST: &str = r#"
### List Wardrobe
Browse the clothing and outfits available to you — your own plus shared project / Quilltap General items.
Format: [[WARDROBE /]]
With filters: [[WARDROBE type_filter="top" /]]"#;

const WARDROBE_READ: &str = r#"
### Read Wardrobe Item
See the full detail of one item (including its Portrait Cue and whether you own it).
By id: [[READ_WARDROBE id="item-uuid" /]]
By name: [[READ_WARDROBE title="Charcoal Sweater" /]]"#;

const WARDROBE_WEAR: &str = r#"
### Wear (put on / layer / swap)
Put on a garment or composite outfit. Works the same for single items and bundles — the item's own settings decide layer-vs-swap.
To wear (honors the item's replace flag — layers it on unless set to replace): [[WEAR mode="wear" id="item-uuid" /]]
By name: [[WEAR mode="wear" title="Charcoal Sweater" /]]
To swap it on, taking off what's there first: [[WEAR mode="replace" id="item-uuid" /]]
To layer into one slot: [[WEAR mode="add_to_slot" slot="top" id="item-uuid" /]]"#;

const WARDROBE_TAKE_OFF: &str = r#"
### Take Off (remove / clear)
Take a worn item off, or empty a slot.
To take off a worn item across every slot it covers: [[TAKE_OFF mode="remove" id="item-uuid" /]]
By name: [[TAKE_OFF mode="remove" title="Charcoal Sweater" /]]
To clear a slot entirely: [[TAKE_OFF mode="clear_slot" slot="top" /]]"#;

const WARDROBE_CREATE: &str = r#"
### Create Wardrobe Item
Design and add a new clothing item to your wardrobe, or gift one to another character. You can also build composite outfits that bundle existing items, and set a Portrait Cue (image_prompt) to steer image generation.
Single garment: [[CREATE_WARDROBE_ITEM title="Red Scarf" types="accessories" appropriateness="casual"]]A soft crimson scarf with golden tassels[[/CREATE_WARDROBE_ITEM]]
Composite outfit (bundles existing items by id or title): [[CREATE_WARDROBE_ITEM title="Rain Outfit" component_titles="Yellow Slicker,Dark Jeans,Wellington Boots"]]Practical attire for a downpour[[/CREATE_WARDROBE_ITEM]]
Gift to another character: [[CREATE_WARDROBE_ITEM title="Red Scarf" types="accessories" recipient="CharacterName"]]A gift for you[[/CREATE_WARDROBE_ITEM]]"#;

const WARDROBE_UPDATE: &str = r#"
### Update Wardrobe Item
Edit an existing item you own (title, description, Portrait Cue, etc.). Only your own items can be edited.
By id: [[UPDATE_WARDROBE_ITEM id="item-uuid" cue="weathered oilskin coat, brass buckles" /]]
By name: [[UPDATE_WARDROBE_ITEM name="Rain Outfit" context="formal" /]]"#;

const WARDROBE_ARCHIVE: &str = r#"
### Archive Wardrobe Item
Retire an item you own — hidden from listings and no longer wearable, but a human can restore it. Only your own items can be archived.
By id: [[ARCHIVE_WARDROBE_ITEM id="item-uuid" /]]
By name: [[ARCHIVE_WARDROBE_ITEM title="Red Scarf" /]]"#;

/// v4 `buildTextBlockInstructions`.
pub fn build_text_block_instructions(options: &TextBlockPromptOptions) -> String {
    let mut tool_docs: Vec<&str> = Vec::new();

    if on(options.whisper) {
        tool_docs.push(WHISPER);
    }
    // `search !== false` — on by default.
    if options.search != Some(false) {
        tool_docs.push(SEARCH);
    }
    if on(options.image_generation) {
        tool_docs.push(IMAGE_GENERATION);
    }
    if on(options.web_search) {
        tool_docs.push(WEB_SEARCH);
    }
    if on(options.state) {
        tool_docs.push(STATE);
    }
    if on(options.rng) {
        tool_docs.push(RNG);
    }
    if on(options.project_info) {
        tool_docs.push(PROJECT_INFO);
    }
    if on(options.help_search) {
        tool_docs.push(HELP_SEARCH);
    }
    if on(options.help_settings) {
        tool_docs.push(HELP_SETTINGS);
    }
    if on(options.help_navigate) {
        tool_docs.push(HELP_NAVIGATE);
    }
    if on(options.create_note) {
        tool_docs.push(CREATE_NOTE);
    }
    if on(options.wardrobe_list) {
        tool_docs.push(WARDROBE_LIST);
    }
    if on(options.wardrobe_read) {
        tool_docs.push(WARDROBE_READ);
    }
    if on(options.wardrobe_wear) {
        tool_docs.push(WARDROBE_WEAR);
    }
    if on(options.wardrobe_take_off) {
        tool_docs.push(WARDROBE_TAKE_OFF);
    }
    if on(options.wardrobe_create) {
        tool_docs.push(WARDROBE_CREATE);
    }
    if on(options.wardrobe_update) {
        tool_docs.push(WARDROBE_UPDATE);
    }
    if on(options.wardrobe_archive) {
        tool_docs.push(WARDROBE_ARCHIVE);
    }

    if tool_docs.is_empty() {
        return String::new();
    }

    let instructions = format!(
        "\n## Available Tools\n\nYou can use the following tools by including special markers in your response. When you want to use a tool, write the marker exactly as shown and I will execute the tool and provide you with the results.\n\n### Marker Format\n- **With content:** [[TOOL_NAME param=\"value\"]]content here[[/TOOL_NAME]]\n- **Self-closing (no content):** [[TOOL_NAME param=\"value\" /]]\n- Parameter values must be in double quotes\n- Tool names are case-insensitive\n{}\n\n## Tool Usage Instructions\n- Place tool markers naturally in your response where you want to use the tool\n- You may use multiple tools in a single response\n- After using a tool, I will provide the results, then you can continue your response\n- Only use tools when they would genuinely help — don't use them unnecessarily\n- Do NOT nest tool markers inside each other\n",
        tool_docs.join("\n")
    );

    js_trim(&instructions).to_string()
}
