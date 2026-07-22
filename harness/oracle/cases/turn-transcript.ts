/**
 * Oracle case: the per-turn transcript builder (P4.6bj unit 1).
 *
 * Drives the REAL functions from the v4 server's
 * lib/services/chat-message/turn-transcript.ts — `buildTurnTranscript` and
 * `findTurnOpenerMessageId` — over the committed shared corpus
 * (harness/oracle/fixtures/turn-transcript.json), and prints one NDJSON row
 * per case on stdout: `{ id, opener, transcript }`, where `transcript` is the
 * raw JSON.stringify of the v4 return value (undefined keys dropped — the
 * Rust differential reproduces exactly that presence shape).
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
  findTurnOpenerMessageId,
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
for (const c of corpus.cases) {
  const messages = c.messages as unknown as MessageEvent[];
  const opener = findTurnOpenerMessageId(messages);
  const transcript = buildTurnTranscript(messages, participants, participantCharacters, {
    turnOpenerMessageId: (c.options.turnOpenerMessageId ?? null) as string | null,
    extractionAnchorMessageId: c.options.extractionAnchorMessageId as string | null | undefined,
    userCharacterId: c.options.userCharacterId as string | undefined,
    userCharacterName: c.options.userCharacterName as string | undefined,
    userCharacterPronouns: c.options.userCharacterPronouns as
      | Character['pronouns']
      | null
      | undefined,
  });
  lines.push(JSON.stringify({ id: c.id, opener, transcript }));
}

process.stdout.write(lines.join('\n') + '\n');
