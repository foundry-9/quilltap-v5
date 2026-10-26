//! Pascal — the custom-tool table (v4 `lib/pascal/`).

pub mod bench_wardrobe;
pub mod custom_tool_schema;
pub mod custom_tool_types;
pub mod custom_tools;
pub mod dice;
pub mod expressions;
// === P4.D329: the Pascal inventory grammar ===
pub mod bench_inventory;
pub mod inventory_aggregate;
pub mod inventory_effects;
pub mod inventory_subjects;
// === LANE-REMOVE (P4.D329): C2 stand-ins — deleted at §S.5 wire 1 ===
pub mod js_value;
pub mod llm_consult;
pub mod metadata_match;
pub mod p4d329_c2_standins;
pub mod placeholders;
pub mod roster;
pub mod run_service;
pub mod side_effects;
pub mod tool_gate;
pub mod tool_vocabulary;
pub mod trigger_phrases;
pub mod trigger_runner;
pub mod wardrobe_effects;
pub mod workbench;
