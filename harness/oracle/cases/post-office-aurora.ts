/**
 * Tier-1 differential oracle for the W4.6b Aurora OUTFIT builders — driving v4's
 * REAL exports so the byte-exact persona / opaque announcement bodies are proven
 * against the Rust port ([`services::aurora_notifications`]).
 *
 * Covers the four outfit builders (opening/change × content/opaque) over the
 * describeOutfit branches (empty / one item / full / accessories-only /
 * multi-per-slot) and a Unicode/em-dash character name. The Core-whisper content
 * builders are already differential-verified in `context_feeders_leaves_equivalence`
 * (W4.6a); the POST functions' `chat_messages` rows are covered by the parent's
 * central tier-3.
 *
 * Pure — runs under `npx tsx` (no DB, no jest). Emits NDJSON to stdout.
 *
 * Generate (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5/harness/oracle/cases/post-office-aurora.ts \
 *       > /tmp/oracle-po-aurora.ndjson
 * Run (in the worktree):
 *   QT_ORACLE_PO_AURORA=/tmp/oracle-po-aurora.ndjson \
 *     cargo test -p quilltap-harness --test post_office_aurora_equivalence
 */

import {
  buildOpeningOutfitContent,
  buildOpeningOutfitOpaqueContent,
  buildOutfitChangeContent,
  buildOutfitChangeOpaqueContent,
} from '@/lib/services/aurora-notifications/writer';

interface OutfitSlotValues {
  top: string[];
  bottom: string[];
  footwear: string[];
  accessories: string[];
}

const cases: Array<{ id: string; characterName: string; outfit: OutfitSlotValues }> = [
  {
    id: 'empty',
    characterName: 'Ada',
    outfit: { top: [], bottom: [], footwear: [], accessories: [] },
  },
  {
    id: 'one-top',
    characterName: 'Bea',
    outfit: { top: ['a blue silk blouse'], bottom: [], footwear: [], accessories: [] },
  },
  {
    id: 'full',
    characterName: 'Cyrus',
    outfit: {
      top: ['a herringbone waistcoat'],
      bottom: ['grey flannel trousers'],
      footwear: ['oxford brogues'],
      accessories: ['a pocket watch', 'a signet ring'],
    },
  },
  {
    id: 'accessories-only',
    characterName: 'Dot',
    outfit: { top: [], bottom: [], footwear: [], accessories: ['a long string of pearls'] },
  },
  {
    id: 'multi-per-slot',
    characterName: 'Ezra',
    outfit: { top: ['a greatcoat', 'a woollen scarf'], bottom: ['a long tweed skirt'], footwear: [], accessories: [] },
  },
  {
    id: 'unicode-name',
    characterName: 'Élodie — the Frankish',
    outfit: { top: ['une chemise blanche'], bottom: [], footwear: [], accessories: [] },
  },
];

async function main(): Promise<void> {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) =>
    out.push(JSON.stringify({ kind, id, value }));

  for (const c of cases) {
    const params = { characterName: c.characterName, outfit: c.outfit };
    emit('opening_content', c.id, buildOpeningOutfitContent(params));
    emit('opening_opaque', c.id, buildOpeningOutfitOpaqueContent(params));
    emit('change_content', c.id, buildOutfitChangeContent(params));
    emit('change_opaque', c.id, buildOutfitChangeOpaqueContent(params));
  }

  for (const line of out) process.stdout.write(line + '\n');
}

void main();
