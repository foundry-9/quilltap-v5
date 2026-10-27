//! The Inventory — the objects a character carries (v4 `lib/inventory/**`,
//! `4fbdad186` + `e1872c94f` + `102a23648` + `7263105c6`'s non-lint half, at
//! `322a5488e`; P4.D324, the keystone).
//!
//! One `.json` file per object under a tier's `Inventory/` folder (a
//! character's vault, a group's or project's official store, Quilltap
//! General); a chat records what each character carries in
//! `chats.equippedInventory`. The storage lives in `db::{inventory_readers,
//! inventory_writes, item_folder_ops, chats_inventory}`; this module holds
//! the model (types, folders, the pool, resolution), the mutations, the
//! pictures, the transfers and the chat-spine chokepoints (equip, arrival
//! defaults, the prompt section). The tools / turn containers
//! (`refs`, `context`), the image phrase and the title lint are sibling
//! lanes' (they add their own `pub mod` lines).

pub mod apply_defaults;
pub mod context;
pub mod dependents;
pub mod equip;
pub mod equipped_set;
pub mod errors;
pub mod folders;
pub mod image_description;
pub mod item_folders;
pub mod item_image_generation;
pub mod item_images;
pub mod item_mutations;
pub mod location;
pub mod pool;
pub mod prompt_section;
pub mod refs;
pub mod resolve;
pub mod roster_gate;
pub mod svg;
pub mod transfers;
pub mod types;
