/**
 * Tier-1 differential oracle for the W4.6a context-feeder PURE leaves — driving
 * v4's REAL exports so the byte-exact builders / formatters / parsers are proven
 * against the Rust port ([`services::off_scene`], [`services::core_whisper`],
 * [`services::memory_recap`], [`services::memory_recap::distill`],
 * [`services::suparna_mail`], [`services::scene_state_tracking`]).
 *
 * Pure-only: no DB. Every function here is a pure v4 export. The stateful reads
 * (frozen-archive ranking against a DB, the recap's vault-summary search, the
 * off-scene SCAN composition, the core-whisper packet assembly) are exercised
 * end-to-end by `build_context_tier3_equivalence`.
 *
 * Pure — runs under `npx tsx` (no DB, no jest). Emits NDJSON to stdout.
 */

import {
  buildOffSceneCharactersContent,
  buildOffSceneCharactersOpaqueContent,
  findIntroducedOffSceneCharacterIds,
} from '@/lib/services/host-notifications/writer';
import {
  resolveCoreWhisperConfig,
  buildCoreWhisperContent,
  buildCoreWhisperOpaqueContent,
  buildCoreWhisperLLMContext,
  type CorePacket,
} from '@/lib/services/aurora-notifications/core-whisper';
import { buildSuparnaMailLLMContext } from '@/lib/services/suparna-notifications/writer';
import { renderRelevantConversationsBlock } from '@/lib/memory/conversation-summary-search';
import { capClothingSummary } from '@/lib/memory/cheap-llm-tasks/image-scene-tasks';

interface OffCard {
  id: string;
  name: string;
  aliases?: string[];
  pronouns?: { subject: string; object: string; possessive: string } | null;
  identity?: string | null;
  description?: string | null;
}

// ---- off-scene content builders ----
const offSceneCases: Array<{ id: string; chars: OffCard[]; user: string | null }> = [
  {
    id: 'single-identity',
    chars: [{ id: 'c1', name: 'Ada', identity: 'A brilliant analyst named {{char}}.' }],
    user: 'Charlie',
  },
  {
    id: 'single-description-fallback',
    chars: [{ id: 'c1', name: 'Bea', description: 'A gentle beekeeper.', identity: '' }],
    user: null,
  },
  {
    id: 'plural-aliases-pronouns-sorted',
    chars: [
      {
        id: 'c2',
        name: 'Zed',
        aliases: ['Z', 'Zee'],
        pronouns: { subject: 'they', object: 'them', possessive: 'theirs' },
        identity: 'The last one.',
      },
      { id: 'c1', name: 'Ada', identity: 'The first one, {{user}} knows her.' },
    ],
    user: 'Charlie',
  },
  {
    id: 'accented-name-sort',
    chars: [
      { id: 'c2', name: 'Zoë', identity: 'z' },
      { id: 'c1', name: 'Ægis', identity: 'a' },
      { id: 'c3', name: 'ada', identity: 'lowercase ada' },
    ],
    user: null,
  },
];

// ---- findIntroducedOffSceneCharacterIds ----
const introducedCases: Array<{ id: string; messages: unknown[] }> = [
  {
    id: 'mixed-host-and-others',
    messages: [
      {
        type: 'message',
        systemSender: 'host',
        systemKind: 'off-scene-characters',
        hostEvent: { introducedCharacterIds: ['c1', 'c2'] },
      },
      { type: 'message', systemSender: 'aurora', systemKind: 'core-whisper' },
      { type: 'message', role: 'USER', content: 'hi' },
      {
        type: 'message',
        systemSender: 'host',
        systemKind: 'off-scene-characters',
        hostEvent: { introducedCharacterIds: ['c2', 'c3'] },
      },
      // Wrong systemKind — ignored.
      {
        type: 'message',
        systemSender: 'host',
        systemKind: 'add',
        hostEvent: { introducedCharacterIds: ['c9'] },
      },
    ],
  },
  { id: 'empty', messages: [] },
];

// ---- core-whisper config precedence ----
const coreConfigCases: Array<{
  id: string;
  chat: { coreWhisperEnabled?: boolean | null; coreWhisperInterval?: number | null } | null;
  character: { coreWhisperEnabled?: boolean | null } | null;
  global:
    | {
        enabled?: boolean;
        interval?: number;
        silenceThreshold?: number;
        packetTokenBudget?: number;
        fireOnContextTransition?: boolean;
      }
    | null;
}> = [
  { id: 'all-defaults', chat: null, character: null, global: null },
  { id: 'chat-disables', chat: { coreWhisperEnabled: false, coreWhisperInterval: 7 }, character: { coreWhisperEnabled: true }, global: null },
  { id: 'character-disables', chat: {}, character: { coreWhisperEnabled: false }, global: null },
  {
    id: 'global-overrides',
    chat: null,
    character: null,
    global: { enabled: false, interval: 20, silenceThreshold: 5, packetTokenBudget: 2048, fireOnContextTransition: false },
  },
  { id: 'chat-null-falls-to-character', chat: { coreWhisperEnabled: null, coreWhisperInterval: null }, character: { coreWhisperEnabled: true }, global: { enabled: false } },
];

// ---- core-whisper content builders (over synthetic packets) ----
const corePackets: Array<{ id: string; packet: CorePacket }> = [
  {
    id: 'own-and-shared',
    packet: {
      files: [
        { path: 'Core/manifesto.md', body: 'I build things.' },
        { path: 'Core/values.md', body: 'Kindness above all.', sourceLabel: 'Household' },
      ],
      approxTokens: 0,
    },
  },
  {
    id: 'single-own',
    packet: { files: [{ path: 'Core/a.md', body: 'One file.' }], approxTokens: 0 },
  },
];

// ---- recap: renderRelevantConversationsBlock ----
const relevantBlockCases: Array<{ id: string; matches: Array<{ conversationId: string; conversationTitle: string; relativePath: string; score: number }> }> = [
  { id: 'empty', matches: [] },
  {
    id: 'two-entries',
    matches: [
      { conversationId: 'cid-1', conversationTitle: 'A Talk', relativePath: 'x', score: 0.9 },
      { conversationId: 'cid-2', conversationTitle: 'Another', relativePath: 'y', score: 0.8 },
    ],
  },
];

// ---- suparna mail llm-context ----
interface Letter {
  path: string;
  from: string;
  sentAt: string;
  body: string;
  alerted: boolean;
  inReplyTo: string | null;
}
const suparnaCases: Array<{ id: string; letters: Letter[] }> = [
  { id: 'empty', letters: [] },
  {
    id: 'single',
    letters: [{ path: 'Mail/friday-2026-02-01.md', from: 'Friday', sentAt: '2026-02-01T09:05:00.000Z', body: '  Hello there.  ', alerted: false, inReplyTo: null }],
  },
  {
    id: 'plural-with-blank',
    letters: [
      { path: 'Mail/a.md', from: 'Ada', sentAt: '2026-02-02T09:00:00.000Z', body: 'First.', alerted: false, inReplyTo: null },
      { path: 'Mail/b.md', from: 'Bob', sentAt: '2026-02-01T09:00:00.000Z', body: '   ', alerted: false, inReplyTo: null },
    ],
  },
];

// ---- scene-state capClothingSummary ----
const clothingCases: Array<{ id: string; value: unknown }> = [
  { id: 'trimmed', value: '  a shirt  ' },
  { id: 'null', value: null },
  { id: 'number', value: 42 },
  { id: 'over-200', value: 'x'.repeat(250) },
  { id: 'exactly-200', value: 'y'.repeat(200) },
];

async function main(): Promise<void> {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) =>
    out.push(JSON.stringify({ kind, id, value }));

  for (const c of offSceneCases) {
    emit('off_scene_content', c.id, buildOffSceneCharactersContent(c.chars as never, c.user));
    emit('off_scene_opaque', c.id, buildOffSceneCharactersOpaqueContent(c.chars as never, c.user));
  }
  for (const c of introducedCases) {
    emit('introduced_ids', c.id, [...findIntroducedOffSceneCharacterIds(c.messages)].sort());
  }
  for (const c of coreConfigCases) {
    emit('core_config', c.id, resolveCoreWhisperConfig(c.chat as never, c.character as never, c.global as never));
  }
  for (const c of corePackets) {
    emit('core_persona', c.id, buildCoreWhisperContent(c.packet));
    emit('core_opaque', c.id, buildCoreWhisperOpaqueContent(c.packet));
    emit('core_llm', c.id, buildCoreWhisperLLMContext(c.packet));
  }
  for (const c of relevantBlockCases) {
    emit('relevant_block', c.id, renderRelevantConversationsBlock(c.matches as never));
  }
  for (const c of suparnaCases) {
    emit('suparna_llm', c.id, buildSuparnaMailLLMContext(c.letters as never));
  }
  for (const c of clothingCases) {
    emit('clothing_cap', c.id, capClothingSummary(c.value));
  }

  for (const line of out) process.stdout.write(line + '\n');
}

void main();
