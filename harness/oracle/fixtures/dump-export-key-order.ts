/**
 * P4.9G4 — dump v4's Zod **schema declaration key order** for every entity the
 * `.qtap` export emits, so the Rust writer can reproduce `JSON.stringify`'s key
 * order byte-for-byte.
 *
 * ── WHY THIS EXISTS ──────────────────────────────────────────────────────────
 * v4 emits each entity as `schema.parse(row)`, and Zod's object parse builds its
 * result by walking the shape in DECLARATION order. So every `.qtap` line's key
 * order is the schema's, not the DB's. v5's read paths marshal in COLUMN order
 * (documented in `db/characters_read.rs`'s header — the rest of the port
 * compares over key-order-independent `Value`s, so it never mattered before).
 * Rather than reorder those landed read paths, the export writer reorders at its
 * own boundary using this table.
 *
 * NOT every export record is schema-ordered: wardrobe items are read straight
 * off the character VAULT (never through `WardrobeItemSchema.parse`), so they
 * carry the vault document's own key order and are deliberately absent here. The
 * doc-store records are assembled key-by-key by the writer itself.
 *
 * The checked-in generator + committed JSON is the standing
 * `byte-exact-static-data-transcription` idiom.
 *
 * Regenerate (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/fixtures/dump-export-key-order.ts \
 *     > $V5W/crates/quilltap-core/src/services/qtap_export/schema-key-order.json
 */

import { z } from 'zod';

async function main(): Promise<void> {
  const types = await import('@/lib/schemas/types');
  const profiles = await import('@/lib/schemas/profile.types');
  const projectTypes = await import('@/lib/schemas/project.types');
  const groupTypes = await import('@/lib/schemas/group.types');
  const tagTypes = await import('@/lib/schemas/tag.types');

  const named: Array<[string, unknown]> = [
    ['character', types.CharacterSchema],
    ['chat', types.ChatMetadataSchema],
    ['chat_message', types.MessageEventSchema],
    ['memory', types.MemorySchema],
    ['roleplay_template', types.RoleplayTemplateSchema],
    ['tag', tagTypes.TagSchema],
    ['project', projectTypes.ProjectSchema],
    ['group', groupTypes.GroupSchema],
    ['connection_profile', profiles.ConnectionProfileSchema],
    ['image_profile', profiles.ImageProfileSchema],
    ['embedding_profile', profiles.EmbeddingProfileSchema],
  ];

  const out: Record<string, string[]> = {};
  for (const [name, schema] of named) {
    const s = schema as z.ZodObject<z.ZodRawShape>;
    if (!s || typeof (s as { shape?: unknown }).shape !== 'object') {
      throw new Error(`schema ${name} is not a ZodObject`);
    }
    out[name] = Object.keys(s.shape);
  }

  process.stdout.write(JSON.stringify(out, null, 2) + '\n');
}

main().catch((err) => {
  process.stderr.write(`dump-export-key-order failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
