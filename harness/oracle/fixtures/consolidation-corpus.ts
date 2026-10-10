/**
 * The consolidation corpus template (P4.D271) — the ONE definition of every
 * holder's memory rows, shared by `build-consolidation-fixture.ts` (which
 * writes them through v4's REAL repositories) and the consolidation oracles
 * (which need the topic markers and the canned-embedding rule).
 *
 * Each holder gets the same ~34-row template under its own id prefix
 * (`000000<hh>…`, so `findByCharacterIdInBatches`' id-ASC order is the template
 * order, ahead of every digest id a run mints):
 *
 *   other:Laura  A1–A4 semantic topic A (plain chat) · DGB an existing
 *                CONSOLIDATED digest (topic B) + B1 (Locked chat), B2 · DGX a
 *                CONSOLIDATED digest with NO embedding · C1–C3 an episode
 *                within one day (Unmoderated chat) · D1–D2 a fresh pair
 *                (below the floor) · S1 a singleton · M1 MANUAL · I1 immature
 *                · K1 cold · N1 no embedding · P1–P3 already considered
 *                (stale) · Q1–Q2 an old pair (the 2×maturity floor of 2)
 *                · A5 an immature topic-A row (matures for the re-run)
 *   self         F1–F3 topic F · F4 immature
 *   none         G1–G3 topic G (G1 links an id that does not exist)
 *   other:orphan H1–H3 topic H (the subject character does not exist)
 *
 * Contents carry `[<topic><n>]` markers; a canned answer is chosen by the
 * topic letter of the first marker in the NOTES. Digest answers carry
 * `(topic X)`, which the canned embedding maps back onto topic X's direction —
 * so a re-run's new rows are attracted to the digest the first run wrote.
 */

export const DAY = 86_400_000;
export const DIMS = 12;
const TOPICS = 'ABCDEFGHIJ';

export function topicIndex(letter: string): number {
  return TOPICS.indexOf(letter);
}

function normalize(v: number[]): number[] {
  const n = Math.sqrt(v.reduce((s, x) => s + x * x, 0));
  return v.map((x) => Math.fround(x / n));
}

/** A row's embedding: its topic's axis plus a small, row-specific tilt. */
export function rowVector(letter: string, n: number): number[] {
  const k = topicIndex(letter);
  const v = new Array<number>(DIMS).fill(0);
  v[k] = 1;
  const tilt = (k + 1 + (n % 5)) % DIMS;
  v[tilt === k ? (k + 6) % DIMS : tilt] += n % 2 === 0 ? 0.18 : -0.18;
  return normalize(v);
}

/** A digest's canned embedding for `(topic X)`. */
export function digestVector(letter: string): number[] {
  const k = topicIndex(letter);
  const v = new Array<number>(DIMS).fill(0);
  v[k] = 1;
  v[(k + 1) % DIMS] = 0.05;
  return normalize(v);
}

/** The canned embedding for any other text: a stable hash-spread vector. */
export function hashVector(text: string): number[] {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  const v = new Array<number>(DIMS).fill(0);
  for (let i = 0; i < DIMS; i++) {
    h = Math.imul(h ^ (h >>> 13), 0x5bd1e995) >>> 0;
    v[i] = (h % 2001) / 1000 - 1;
  }
  return normalize(v);
}

/** The canned embedding rule both oracles share. `null` = fail the embed. */
export function cannedEmbeddingFor(text: string): number[] | null {
  if (text.includes('FAILEMBED')) return null;
  const m = /\(topic ([A-J])\)/.exec(text);
  return m ? digestVector(m[1]) : hashVector(text);
}

export function memoryId(holderIndex: number, row: number): string {
  const hh = holderIndex.toString(16).padStart(2, '0');
  // `000000<hh>` so the template rows sort (id ASC — the run's load order)
  // ahead of any digest id a run mints at random: bucket order is first-seen.
  return `000000${hh}-0000-4000-8000-${row.toString(16).padStart(12, '0')}`;
}

export interface TemplateRow {
  row: number;
  tag: string;
  bucket: 'other' | 'self' | 'none' | 'orphan';
  topic: string | null;
  source: 'AUTO' | 'MANUAL' | 'CONSOLIDATED';
  kind: 'semantic' | 'episodic';
  createdAgoDays: number;
  occurredAt?: string;
  narrativeTime?: string;
  chat?: 'plain' | 'locked' | 'unmoderated';
  tier?: 'hot' | 'cold';
  consideredAgoDays?: number;
  importance: number;
  reinforcedImportance: number;
  reinforcementCount: number;
  related?: string[];
  content: string;
  keywords: string[];
  entities?: string[];
  witnessedContext?: string;
  consolidatedFrom?: string[];
  noEmbedding?: boolean;
}

const KW = ['present', 'scope: wide', 'information'];

export const TEMPLATE: TemplateRow[] = [
  { row: 1, tag: 'A1', bucket: 'other', topic: 'A', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, chat: 'plain', importance: 0.5, reinforcedImportance: 0.55, reinforcementCount: 2, related: ['B1', 'S1', 'A2'], content: '[A1] Laura keeps a chart of the northern coast.', keywords: ['charts', ...KW], entities: ['Laura'], witnessedContext: 'user_present' },
  { row: 2, tag: 'A2', bucket: 'other', topic: 'A', source: 'AUTO', kind: 'semantic', createdAgoDays: 19, chat: 'plain', importance: 0.6, reinforcedImportance: 0.6, reinforcementCount: 1, content: '[A2] Laura prefers tea to coffee now.', keywords: ['tea', 'present', 'scope: narrow', 'mannerisms'] },
  { row: 3, tag: 'A3', bucket: 'other', topic: 'A', source: 'AUTO', kind: 'semantic', createdAgoDays: 18, chat: 'plain', importance: 0.4, reinforcedImportance: 0.45, reinforcementCount: 3, content: '[A3] Laura once preferred coffee.', keywords: ['coffee', 'past', 'scope: narrow', 'mannerisms'], entities: ['Laura', 'Coffee House'] },
  { row: 4, tag: 'A4', bucket: 'other', topic: 'A', source: 'AUTO', kind: 'semantic', createdAgoDays: 17, chat: 'plain', importance: 0.7, reinforcedImportance: 0.7, reinforcementCount: 1, content: '[A4] Laura drew a new map of the fjords.\nIt took a week.', keywords: ['maps', ...KW] },
  { row: 5, tag: 'DGB', bucket: 'other', topic: 'B', source: 'CONSOLIDATED', kind: 'semantic', createdAgoDays: 40, consideredAgoDays: 30, importance: 0.6, reinforcedImportance: 0.7, reinforcementCount: 5, content: '(topic B) Laura sails a small boat named the Kittiwake.', keywords: ['boat', ...KW], consolidatedFrom: ['0dee0000-0000-4000-8000-000000000001', '0dee0000-0000-4000-8000-000000000002'], related: ['A4'] },
  { row: 6, tag: 'B1', bucket: 'other', topic: 'B', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, chat: 'locked', importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[B1] Laura repainted the Kittiwake blue.', keywords: ['boat', ...KW] },
  { row: 7, tag: 'B2', bucket: 'other', topic: 'B', source: 'AUTO', kind: 'semantic', createdAgoDays: 21, chat: 'plain', importance: 0.5, reinforcedImportance: 0.52, reinforcementCount: 2, related: ['C1'], content: '[B2] Laura moors the Kittiwake at the east quay.', keywords: ['quay', ...KW] },
  { row: 8, tag: 'DGX', bucket: 'other', topic: 'B', source: 'CONSOLIDATED', kind: 'semantic', createdAgoDays: 40, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 2, content: '(topic B) An unembedded digest.', keywords: KW, noEmbedding: true },
  { row: 9, tag: 'C1', bucket: 'other', topic: 'C', source: 'AUTO', kind: 'episodic', createdAgoDays: 30, occurredAt: '2026-09-01T10:00:00.000Z', narrativeTime: 'the long dusk', chat: 'unmoderated', importance: 0.6, reinforcedImportance: 0.6, reinforcementCount: 1, content: '[C1] Laura and the holder walked the sea wall.', keywords: ['walk', 'past', 'scope: wide', 'history'] },
  { row: 10, tag: 'C2', bucket: 'other', topic: 'C', source: 'AUTO', kind: 'episodic', createdAgoDays: 30, occurredAt: '2026-09-01T18:00:00.000Z', chat: 'unmoderated', importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[C2] A storm caught them at the lighthouse.', keywords: ['storm', 'past', 'scope: wide', 'history'] },
  { row: 11, tag: 'C3', bucket: 'other', topic: 'C', source: 'AUTO', kind: 'episodic', createdAgoDays: 29, occurredAt: '2026-09-01T23:00:00Z', chat: 'unmoderated', importance: 0.7, reinforcedImportance: 0.7, reinforcementCount: 2, content: '[C3] They sheltered in the keeper\'s cottage.', keywords: ['cottage', 'moment', 'scope: wide', 'history'] },
  { row: 12, tag: 'D1', bucket: 'other', topic: 'D', source: 'AUTO', kind: 'semantic', createdAgoDays: 9, importance: 0.3, reinforcedImportance: 0.3, reinforcementCount: 1, content: '[D1] Laura hums sea shanties.', keywords: KW },
  { row: 13, tag: 'D2', bucket: 'other', topic: 'D', source: 'AUTO', kind: 'semantic', createdAgoDays: 9, importance: 0.3, reinforcedImportance: 0.31, reinforcementCount: 1, content: '[D2] Laura knows every verse of the Drunken Sailor.', keywords: KW },
  { row: 14, tag: 'S1', bucket: 'other', topic: 'E', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.4, reinforcedImportance: 0.4, reinforcementCount: 1, related: ['A3'], content: '[E1] Laura owns a grey cat.', keywords: KW },
  { row: 15, tag: 'M1', bucket: 'other', topic: 'A', source: 'MANUAL', kind: 'semantic', createdAgoDays: 25, importance: 0.9, reinforcedImportance: 0.9, reinforcementCount: 1, content: '[A9] A hand-written note about Laura\'s charts.', keywords: KW },
  { row: 16, tag: 'I1', bucket: 'other', topic: 'A', source: 'AUTO', kind: 'semantic', createdAgoDays: 1, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[A7] Laura bought new chart paper.', keywords: KW },
  { row: 17, tag: 'K1', bucket: 'other', topic: 'A', source: 'AUTO', kind: 'semantic', createdAgoDays: 50, tier: 'cold', importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, related: ['A1'], content: '[A8] A cold note about Laura\'s first chart.', keywords: KW },
  { row: 18, tag: 'N1', bucket: 'other', topic: null, source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: 'A note with no embedding.', keywords: KW, noEmbedding: true },
  { row: 19, tag: 'F1', bucket: 'self', topic: 'F', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.6, reinforcedImportance: 0.6, reinforcementCount: 1, content: '[F1] I keep the Estate ledgers.', keywords: ['ledger', ...KW] },
  { row: 20, tag: 'F2', bucket: 'self', topic: 'F', source: 'AUTO', kind: 'semantic', createdAgoDays: 19, importance: 0.5, reinforcedImportance: 0.55, reinforcementCount: 2, content: '[F2] I balance the ledgers every Sunday.', keywords: ['ledger', 'present', 'scope: wide', 'mannerisms'] },
  { row: 21, tag: 'F3', bucket: 'self', topic: 'F', source: 'AUTO', kind: 'semantic', createdAgoDays: 18, importance: 0.4, reinforcedImportance: 0.4, reinforcementCount: 1, content: '[F3] I write the ledgers in green ink.', keywords: KW },
  { row: 22, tag: 'G1', bucket: 'none', topic: 'G', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, related: ['0dee0000-0000-4000-8000-00000000dead'], content: '[G1] The harbour bell rings at noon.', keywords: KW },
  { row: 23, tag: 'G2', bucket: 'none', topic: 'G', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[G2] The bell was cast in 1802.', keywords: KW },
  { row: 24, tag: 'G3', bucket: 'none', topic: 'G', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[G3] The bell-ringer is called Amos.', keywords: KW },
  { row: 25, tag: 'H1', bucket: 'orphan', topic: 'H', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[H1] The vanished one liked figs.', keywords: KW },
  { row: 26, tag: 'H2', bucket: 'orphan', topic: 'H', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[H2] The vanished one grew figs.', keywords: KW },
  { row: 27, tag: 'H3', bucket: 'orphan', topic: 'H', source: 'AUTO', kind: 'semantic', createdAgoDays: 20, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[H3] The vanished one sold figs.', keywords: KW },
  { row: 28, tag: 'P1', bucket: 'other', topic: 'I', source: 'AUTO', kind: 'semantic', createdAgoDays: 30, consideredAgoDays: 1, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[I1] Laura reads by lamplight.', keywords: KW },
  { row: 29, tag: 'P2', bucket: 'other', topic: 'I', source: 'AUTO', kind: 'semantic', createdAgoDays: 30, consideredAgoDays: 1, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[I2] Laura reads until midnight.', keywords: KW },
  { row: 30, tag: 'P3', bucket: 'other', topic: 'I', source: 'AUTO', kind: 'semantic', createdAgoDays: 30, consideredAgoDays: 1, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[I3] Laura reads poetry aloud.', keywords: KW },
  { row: 31, tag: 'Q1', bucket: 'other', topic: 'J', source: 'AUTO', kind: 'semantic', createdAgoDays: 30, importance: 0.4, reinforcedImportance: 0.4, reinforcementCount: 1, content: '[J1] Laura distrusts barometers.', keywords: KW },
  { row: 32, tag: 'Q2', bucket: 'other', topic: 'J', source: 'AUTO', kind: 'semantic', createdAgoDays: 31, importance: 0.4, reinforcedImportance: 0.42, reinforcementCount: 1, content: '[J2] Laura says barometers lie.', keywords: KW },
  { row: 33, tag: 'F4', bucket: 'self', topic: 'F', source: 'AUTO', kind: 'semantic', createdAgoDays: 1, importance: 0.5, reinforcedImportance: 0.5, reinforcementCount: 1, content: '[F4] I bought a new ledger.', keywords: KW },
  { row: 34, tag: 'A5', bucket: 'other', topic: 'A', source: 'AUTO', kind: 'semantic', createdAgoDays: 1, importance: 0.6, reinforcedImportance: 0.6, reinforcementCount: 1, content: '[A6] Laura framed her best chart.', keywords: KW },
];

/** The template's row number for a tag (links name tags). */
export function rowOf(tag: string): number {
  const r = TEMPLATE.find((t) => t.tag === tag);
  if (!r) throw new Error(`no template row ${tag}`);
  return r.row;
}
