/**
 * Oracle case (P4.D377 item 7 — v4 `600c2906f`, the wardrobe Duplicate): the
 * server-side item draft.
 *
 * Drives the REAL `@/lib/wardrobe/item-draft` (`draftFromItem`,
 * `draftToPayload`) — client-safe, no mocks, no reimplementation. The corpus
 * walks every truthiness guard: each optional field null / absent / blank /
 * whitespace-only (incl. U+FEFF, which JS `.trim()` strips, and U+0085, which it
 * does not), a padded title, a composite with and without `replace`, a leaf
 * carrying `replace: true`, `pronounGuidance` present / absent / empty, a
 * folder, `isDefault` absent / true. `payloadJson` is the byte form (key
 * order).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-item-draft.ts \
 *     > /tmp/oracle-wardrobe-item-draft.ndjson
 */

import { draftFromItem, draftToPayload } from '@/lib/wardrobe/item-draft';
import type { WardrobeItem } from '@/lib/schemas/wardrobe.types';

const BASELINE = 'p4.d377-wardrobe-item-draft';

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

emit({ kind: 'meta', baseline: BASELINE });

const base = {
  id: '00000000-0000-4000-8000-000000000001',
  title: 'Linen Shirt',
  types: ['top'],
  folder: '',
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
};

const rows: Array<[string, Record<string, unknown>]> = [];
const add = (id: string, over: Record<string, unknown>, drop: string[] = []) => {
  const item: Record<string, unknown> = { ...base, ...over };
  for (const k of drop) delete item[k];
  rows.push([id, item]);
};

// ── the minimal item and the absent / null legs ─────────────────────────────
add('minimal_absent_everything', {});
add('all_null', {
  description: null,
  imagePrompt: null,
  appropriateness: null,
  componentItemIds: null,
  pronounGuidance: null,
  folder: null,
  isDefault: null,
  replace: null,
});
add('all_present', {
  description: 'A plain linen shirt.',
  imagePrompt: 'linen shirt, studio light',
  appropriateness: 'casual',
  isDefault: true,
  pronounGuidance: ['she/her'],
  folder: 'Tops/Summer',
});

// ── blank / whitespace text (blankToNull over JS .trim()) ───────────────────
add('blank_strings', { description: '', imagePrompt: '', appropriateness: '' });
add('space_only', { description: '   ', imagePrompt: '\t\n', appropriateness: ' \r ' });
add('feff_only', { description: '﻿', imagePrompt: ' ﻿ ', appropriateness: ' ' });
add('nel_only_kept', { description: '\u0085', imagePrompt: 'x\u0085', appropriateness: ' ' });
add('padded_text_kept_raw', { description: '  padded  ', imagePrompt: '\tprompt\t' });
add('ideographic_space', { description: '　', appropriateness: '　ok' });

// ── the title (draft.title.trim()) ──────────────────────────────────────────
add('title_padded', { title: '  Linen Shirt  ' });
add('title_feff', { title: '﻿Hat﻿' });
add('title_nel_kept', { title: '\u0085Hat' });
add('title_copy_suffix', { title: 'Linen Shirt (copy 2)' });

// ── composite and replace (isComposite) ─────────────────────────────────────
add('leaf_replace_true_dropped', { replace: true });
add('leaf_empty_components_replace_true', { componentItemIds: [], replace: true });
add('composite_replace_true', {
  componentItemIds: ['00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-0000000000a2'],
  replace: true,
  types: ['top', 'bottom'],
});
add('composite_replace_false', {
  componentItemIds: ['00000000-0000-4000-8000-0000000000a1'],
  replace: false,
});
add('composite_replace_absent', { componentItemIds: ['00000000-0000-4000-8000-0000000000a1'] }, ['replace']);

// ── pronoun guidance / folder / types ───────────────────────────────────────
add('pronouns_empty', { pronounGuidance: [] });
add('pronouns_two', { pronounGuidance: ['he/him', 'they/them'] });
add('folder_nested', { folder: 'Coats/Winter/Heavy' });
add('types_many', { types: ['footwear', 'top', 'bottom', 'accessories'] });
add('types_empty', { types: [] });
add('is_default_false', { isDefault: false });
add('archived_item', { archivedAt: '2026-02-02T00:00:00.000Z' });
add('astral_text', { title: '🧥 Coat', description: '𝒜 cloak' });

for (const [id, item] of rows) {
  const draft = draftFromItem(item as unknown as WardrobeItem);
  const payload = draftToPayload(draft);
  emit({ kind: 'case', id, item, draft, payload, payloadJson: JSON.stringify(payload) });
}
