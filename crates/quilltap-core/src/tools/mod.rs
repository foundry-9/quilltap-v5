//! The tool-handler family (v4 `lib/tools/`). Wave-4 (W4.1) begins here with the
//! RNG tool — the first handler ported. The tool loops, the registry, and the
//! provider-facing `buildTools` slate are later W4.1 sub-units; this module holds
//! the executable handlers themselves.

pub mod annotations;
pub mod ask_carina;
pub mod definitions;
pub mod discard_mail;
pub mod doc_edit;
pub mod executor;
pub mod generate_image;
pub mod help;
pub mod help_search;
pub mod inventory_archive;
pub mod inventory_create;
pub mod inventory_equip;
pub mod inventory_give;
pub mod inventory_list;
pub mod inventory_pick_up;
pub mod inventory_read;
pub mod inventory_shared;
pub mod inventory_tool_flags;
pub mod inventory_tool_table;
pub mod inventory_unequip;
pub mod inventory_update;
pub mod list_mail;
pub mod llm_number;
pub mod log_create_entry; // P4.D331
pub mod log_grep; // P4.D331
pub mod log_read; // P4.D331
pub mod native_tool_prompt;
pub mod photo;
pub mod project_info;
pub mod pseudo_tool_support;
pub mod read_conversation;
pub mod read_mail;
pub mod request_full_context;
pub mod rng;
pub mod run_custom;
pub mod run_sql;
pub mod search;
pub mod self_inventory;
pub mod send_mail;
pub mod simple_json_parser;
pub mod simple_json_prompt;
pub mod state;
pub mod terminal;
pub mod text_block_parser;
pub mod text_block_prompt;
pub mod wardrobe_archive;
pub mod wardrobe_create;
pub mod wardrobe_history;
pub mod wardrobe_list;
pub mod wardrobe_read;
pub mod wardrobe_rollback;
pub mod wardrobe_shared;
pub mod wardrobe_take_off;
pub mod wardrobe_tool_table;
pub mod wardrobe_update;
pub mod wardrobe_wear;
pub mod web_search;
pub mod whisper;
