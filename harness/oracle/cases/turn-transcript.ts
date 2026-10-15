/**
 * Oracle case: the per-turn transcript builder (P4.6bj unit 1; grown for
 * bug 245 by P4.D296 Tier 1 #4 — pin `83334b9a5`).
 *
 * Drives the REAL functions from the v4 server's
 * lib/services/chat-message/turn-transcript.ts — `buildTurnTranscript` (now
 * with `extractAfterMessageId`), `findTurnOpenerMessageId`,
 * `listExtractionTurns`, `withPriorAsMaterial` and
 * `findAutonomousTurnAnchorMessageId` — over the committed shared corpus
 * (harness/oracle/fixtures/turn-transcript.json), and prints one NDJSON row
 * per case on stdout: `{ id, opener, transcript }` for the builder cases,
 * `{ id, kind: 'turns' | 'anchor' | 'material', out }` for the rest, where
 * each value is the raw JSON.stringify of the v4 return value (undefined
 * keys dropped — the Rust differential reproduces exactly that presence
 * shape).
 *
 * IMPORTANT — this imports the actual app code, it does not reimplement it.
 * Run from inside the server checkout so `@/` path aliases resolve:
 *
 *   cd ~/source/quilltap-server
 *   npx tsx <worktree>/harness/oracle/cases/turn-transcript.ts > /tmp/oracle-turn-transcript.ndjson
 *
 * The corpus is fully pinned (no randomness, no clocks); both sides read the
 * SAME committed fixture file.
 */

import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  buildTurnTranscript,
  findAutonomousTurnAnchorMessageId,
  findTurnOpenerMessageId,
  listExtractionTurns,
  withPriorAsMaterial,
} from '@/lib/services/chat-message/turn-transcript';
import type { Character, ChatParticipantBase, MessageEvent } from '@/lib/schemas/types';

interface CorpusCase {
  id: string;
  messages: Record<string, unknown>[];
  options: Record<string, unknown>;
}

interface Corpus {
  characters: Record<string, Record<string, unknown>>;
  participants: Record<string, unknown>[];
  cases: CorpusCase[];
  turnLists: { id: string; chatType: string | null; messages: Record<string, unknown>[] }[];
  anchorCases: { id: string; messages: Record<string, unknown>[] }[];
  materialCases: string[];
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'turn-transcript.json'), 'utf8'),
) as Corpus;

const participantCharacters = new Map<string, Character>(
  Object.entries(corpus.characters) as unknown as [string, Character][],
);
const participants = corpus.participants as unknown as ChatParticipantBase[];

const lines: string[] = [];
const built = new Map<string, ReturnType<typeof buildTurnTranscript>>();
for (const c of corpus.cases) {
  const messages = c.messages as unknown as MessageEvent[];
  const opener = findTurnOpenerMessageId(messages);
  const transcript = buildTurnTranscript(messages, participants, participantCharacters, {
    turnOpenerMessageId: (c.options.turnOpenerMessageId ?? null) as string | null,
    extractionAnchorMessageId: c.options.extractionAnchorMessageId as string | null | undefined,
    extractAfterMessageId: c.options.extractAfterMessageId as string | null | undefined,
    userCharacterId: c.options.userCharacterId as string | undefined,
    userCharacterName: c.options.userCharacterName as string | undefined,
    userCharacterPronouns: c.options.userCharacterPronouns as
      | Character['pronouns']
      | null
      | undefined,
  });
  built.set(c.id, transcript);
  lines.push(JSON.stringify({ id: c.id, opener, transcript }));
}
for (const t of corpus.turnLists) {
  const out = listExtractionTurns(t.messages as unknown as MessageEvent[], t.chatType);
  lines.push(JSON.stringify({ id: t.id, kind: 'turns', out }));
}
for (const a of corpus.anchorCases) {
  const out = findAutonomousTurnAnchorMessageId(a.messages as unknown as MessageEvent[]);
  lines.push(JSON.stringify({ id: a.id, kind: 'anchor', out }));
}
for (const id of corpus.materialCases) {
  const out = withPriorAsMaterial(built.get(id)!);
  lines.push(JSON.stringify({ id: `material_${id}`, kind: 'material', out }));
}

process.stdout.write(lines.join('\n') + '\n');
