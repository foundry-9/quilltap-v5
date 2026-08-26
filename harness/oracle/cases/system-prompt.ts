/**
 * Oracle case: system-prompt builder (chat-orchestration wave 2.1).
 *
 * Drives the REAL exported functions from
 * v4's lib/chat/context/system-prompt-builder.ts:
 *   buildIdentityStack, buildPublicIdentityCard, buildSystemPrompt,
 *   buildOtherParticipantsInfo, buildIdentityReinforcement.
 *
 * All five are pure. `buildSystemPrompt` reaches through to
 * calculateCurrentTimestamp (which reads Date.now()) only on the `{{timestamp}}`
 * template path (timestampConfig fires AND autoPrepend === false), so the clock
 * is pinned exactly the way harness/oracle/cases/chat-timestamp.ts does, and the
 * host offset the undefined-timezone path reads is captured. Run under TZ=UTC so
 * getTimezoneOffset() === 0 (only relevant for the timezone === undefined rows).
 *
 * Run from inside the server checkout (TZ=UTC required):
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/system-prompt.ts \
 *     > /tmp/oracle-system-prompt.ndjson
 */

import {
  buildIdentityStack,
  buildPublicIdentityCard,
  buildSystemPrompt,
  buildOtherParticipantsInfo,
  buildIdentityReinforcement,
  type OtherParticipantInfo,
} from '@/lib/chat/context/system-prompt-builder'
import type { Character, ChatParticipantBase, TimestampConfig } from '@/lib/schemas/types'

// Pin the wall clock (mirrors chat-timestamp.ts). Only buildSystemPrompt's
// {{timestamp}} path reads it, but pinning globally keeps every row deterministic.
let PINNED_NOW = 0
const RealDate = Date
class MockDate extends RealDate {
  constructor(...args: any[]) {
    if (args.length === 0) {
      super(PINNED_NOW)
    } else {
      // @ts-expect-error variadic forward to the native Date constructor
      super(...args)
    }
  }
  static now(): number {
    return PINNED_NOW
  }
}
;(globalThis as any).Date = MockDate

const LOCAL_OFFSET_MIN = new Date(0).getTimezoneOffset()
if (LOCAL_OFFSET_MIN !== 0) {
  throw new Error(`system-prompt oracle must run under TZ=UTC (getTimezoneOffset=${LOCAL_OFFSET_MIN})`)
}

// A fixed instant used by the {{timestamp}} rows.
const T_MAIN = new RealDate('2026-07-02T12:34:56.789Z').getTime()

// The wire shapes emitted per case kind. The Rust harness deserializes these and
// reconstructs the core inputs, then diffs `out` exactly.
type IdentityStackRow = {
  kind: 'identityStack'
  id: string
  character: any
  userCharacter?: { name: string; description: string } | null
  selectedSystemPromptId?: string | null
  scenarioText?: string | null
  out: string
}

type PublicCardRow = {
  kind: 'publicCard'
  id: string
  character: any
  userName?: string | null
  out: string
}

type SystemPromptRow = {
  kind: 'systemPrompt'
  id: string
  character: any
  userCharacter?: { name: string; description: string } | null
  roleplayTemplate?: { systemPrompt: string } | null
  toolInstructions?: string | null
  selectedSystemPromptId?: string | null
  timestampConfig?: TimestampConfig | null
  isInitialMessage?: boolean
  timezone?: string | null
  scenarioText?: string | null
  precompiledIdentityStack?: string | null
  /** P4.D50 (v4 `7df7de8e`): the instance-wide Taboo phrases. `null` is v4's
   *  OMITTED option (no section); `[]` is the explicit-empty arm. */
  tabooPhrases?: string[] | null
  /** P4.D103 (v4 `8f868109`): the rendered standing-instructions section.
   *  `null` is v4's OMITTED option (no section). */
  standingInstructions?: string | null
  nowMs: number
  localOffsetMin: number
  out: string
}

type OtherParticipantsRow = {
  kind: 'otherParticipants'
  id: string
  respondingParticipantId: string
  participants: any[]
  characters: Record<string, any>
  out: OtherParticipantInfo[]
}

type ReinforcementRow = {
  kind: 'reinforcement'
  id: string
  characterName: string
  out: string
}

type Row =
  | IdentityStackRow
  | PublicCardRow
  | SystemPromptRow
  | OtherParticipantsRow
  | ReinforcementRow

const rows: Row[] = []

// --- Character builder ------------------------------------------------------
// Only the fields the builder consumes; a minimal `id`/`userId` keep the type
// happy. Optional fields are omitted unless a case sets them.
function ch(overrides: Partial<Character> & { name: string }): Character {
  return {
    id: '00000000-0000-4000-8000-000000000000',
    userId: '00000000-0000-4000-8000-000000000001',
    name: overrides.name,
    scenarios: overrides.scenarios ?? [],
    systemPrompts: overrides.systemPrompts ?? [],
    isFavorite: false,
    npc: false,
    controlledBy: 'llm',
    tags: [],
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...overrides,
  } as Character
}

function phys(overrides: Record<string, any>): any {
  return {
    id: '00000000-0000-4000-8000-0000000000aa',
    name: overrides.name ?? 'Default look',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...overrides,
  }
}

// ==== buildIdentityStack ====================================================
{
  const cases: Array<[string, Character, any, string | null | undefined, string | null | undefined]> = [
    ['minimal', ch({ name: 'Ada' }), null, undefined, undefined],
    ['manifesto-only', ch({ name: 'Ada', manifesto: 'The core truth.' }), null, undefined, undefined],
    ['personality-only', ch({ name: 'Ada', personality: 'Curious and dry.' }), null, undefined, undefined],
    ['description-in-context', ch({ name: 'Ada', description: 'Fidgets with gears.', personality: '{{char}} is {{description}}' }), null, undefined, undefined],
    [
      'full-no-prompt',
      ch({
        name: 'Ada',
        manifesto: 'Manifesto of {{char}}.',
        personality: 'Curious.',
        aliases: ['The Inventor', 'Cog'],
        pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
        exampleDialogues: '{{user}}: hi\n{{char}}: hello, {{user}}',
      }),
      { name: 'Nadia', description: 'a wanderer' },
      'A {{char}}-only scene',
      undefined,
    ],
    [
      'selected-prompt',
      ch({
        name: 'Ada',
        systemPrompts: [
          { id: 'p1', name: 'A', content: 'Selected prompt for {{char}}.', isDefault: false },
          { id: 'p2', name: 'B', content: 'Default prompt.', isDefault: true },
        ] as any,
      }),
      null,
      undefined,
      'p1',
    ],
    [
      'default-prompt-fallback',
      ch({
        name: 'Ada',
        systemPrompts: [
          { id: 'p1', name: 'A', content: 'Non-default.', isDefault: false },
          { id: 'p2', name: 'B', content: 'The default for {{char}}.', isDefault: true },
        ] as any,
      }),
      null,
      undefined,
      undefined,
    ],
    [
      'selected-missing-falls-to-default',
      ch({
        name: 'Ada',
        systemPrompts: [{ id: 'p2', name: 'B', content: 'The default.', isDefault: true }] as any,
      }),
      null,
      undefined,
      'nonexistent',
    ],
    [
      'no-prompts-array-empty',
      ch({ name: 'Ada', systemPrompts: [] as any }),
      null,
      undefined,
      'p1',
    ],
    [
      'phys-short',
      ch({ name: 'Ada', physicalDescription: phys({ name: 'Portrait', shortPrompt: 'brass goggles', usageContext: 'close-up' }) }),
      null,
      undefined,
      undefined,
    ],
    [
      'phys-fallback-chain',
      ch({ name: 'Ada', physicalDescription: phys({ name: 'Look', shortPrompt: '', mediumPrompt: '', longPrompt: 'a long description' }) }),
      null,
      undefined,
      undefined,
    ],
    [
      'phys-fulldescription-only',
      ch({ name: 'Ada', physicalDescription: phys({ name: 'Look', fullDescription: 'entire body copy' }) }),
      null,
      undefined,
      undefined,
    ],
    [
      'phys-no-text-skipped',
      ch({ name: 'Ada', physicalDescription: phys({ name: 'Empty' }) }),
      null,
      undefined,
      undefined,
    ],
    [
      'scenario-from-array',
      ch({ name: 'Ada', personality: 'Scene: {{scenario}}', scenarios: [{ id: 's1', title: 'T', content: 'array scenario body', createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-01T00:00:00.000Z' }] as any }),
      null,
      undefined,
      undefined,
    ],
    // [P4.D120 / v4 `d25dacc1`] An ARCHIVED scenario at index 0 must not become
    // the implicit scene — `firstActiveScenarioContent`, the same rule as "an
    // archived file can't be the folder's default".
    [
      'scenario-array-skips-archived',
      ch({ name: 'Ada', personality: 'Scene: {{scenario}}', scenarios: [
        { id: 's0', title: 'Shelved', content: 'archived body', archived: true, createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-01T00:00:00.000Z' },
        { id: 's1', title: 'T', content: 'the first ACTIVE body', createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-01T00:00:00.000Z' },
      ] as any }),
      null,
      undefined,
      undefined,
    ],
    [
      'scenario-array-all-archived-is-empty',
      ch({ name: 'Ada', personality: 'Scene: [{{scenario}}]', scenarios: [
        { id: 's0', title: 'Shelved', content: 'archived body', archived: true, createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-01T00:00:00.000Z' },
      ] as any }),
      null,
      undefined,
      undefined,
    ],
    [
      'scenario-override-beats-array',
      ch({ name: 'Ada', personality: 'Scene: {{scenario}}', scenarios: [{ id: 's1', title: 'T', content: 'array body', createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-01T00:00:00.000Z' }] as any }),
      null,
      'override body',
      undefined,
    ],
    [
      'nonascii',
      ch({
        name: 'Élise',
        manifesto: '{{char}} habite à Genève. 🎩',
        aliases: ['Élisa', 'la voyageuse'],
        pronouns: { subject: 'elle', object: 'la', possessive: 'sa' },
      }),
      { name: 'Zoë', description: 'une voyageuse' },
      'скеналио с {{char}}',
      undefined,
    ],
    [
      'empty-name-user-fallback',
      ch({ name: 'Ada', personality: 'talking to {{user}}' }),
      { name: '', description: '' },
      undefined,
      undefined,
    ],
  ]
  for (const [id, character, userCharacter, scenarioText, selectedSystemPromptId] of cases) {
    const out = buildIdentityStack({ character, userCharacter, scenarioText, selectedSystemPromptId })
    rows.push({ kind: 'identityStack', id, character, userCharacter, scenarioText, selectedSystemPromptId, out })
  }
}

// ==== buildPublicIdentityCard ==============================================
{
  const cases: Array<[string, Character, string | null | undefined]> = [
    ['name-only-fallback', ch({ name: 'Ada' }), undefined],
    ['identity-preferred', ch({ name: 'Ada', identity: 'A wandering inventor.', description: 'Fidgets.' }), undefined],
    ['description-fallback', ch({ name: 'Ada', description: 'Fidgets with gears.' }), undefined],
    ['whitespace-identity-falls-to-description', ch({ name: 'Ada', identity: '   \n ', description: 'Fidgets.' }), undefined],
    ['both-empty-fallback', ch({ name: 'Ada', identity: '', description: '' }), undefined],
    ['title', ch({ name: 'Ada', title: 'the rival', identity: 'Public face.' }), undefined],
    ['title-template', ch({ name: 'Ada', title: 'friend of {{user}}', identity: 'Public.' }), 'Bram'],
    ['title-whitespace-skipped', ch({ name: 'Ada', title: '   ', identity: 'Public.' }), undefined],
    ['pronouns', ch({ name: 'Ada', pronouns: { subject: 'she', object: 'her', possessive: 'hers' }, identity: 'Public.' }), undefined],
    ['aliases', ch({ name: 'Ada', aliases: ['Cog', 'The Inventor'], identity: 'Public.' }), undefined],
    [
      'all-fields',
      ch({
        name: 'Ada',
        title: 'the rival',
        pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
        aliases: ['Cog'],
        identity: '{{char}} is known to {{user}}.',
        description: 'private acquaintance view',
      }),
      'Bram',
    ],
    ['identity-with-template', ch({ name: 'Ada', identity: '{{char}} greets {{user}}.' }), 'Bram'],
    ['user-name-empty-falls-to-User', ch({ name: 'Ada', identity: 'sees {{user}}' }), ''],
    ['nonascii', ch({ name: 'Élise', title: 'la rivale', aliases: ['Élisa'], pronouns: { subject: 'elle', object: 'la', possessive: 'sa' }, identity: 'Connue de {{user}}. 🎩' }), 'Zoë'],
  ]
  for (const [id, character, userName] of cases) {
    const out = buildPublicIdentityCard(character, userName)
    rows.push({ kind: 'publicCard', id, character, userName, out })
  }
}

// ==== buildSystemPrompt ====================================================
function baseTsConfig(overrides: Partial<TimestampConfig> = {}): TimestampConfig {
  return {
    mode: 'EVERY_MESSAGE',
    format: 'FRIENDLY',
    customFormat: null,
    useFictionalTime: false,
    fictionalBaseTimestamp: null,
    fictionalBaseRealTime: null,
    autoPrepend: true,
    timezone: null,
    intervalMinutes: 15,
    ...overrides,
  } as TimestampConfig
}

function pushSystemPrompt(
  id: string,
  opts: {
    character: Character
    userCharacter?: { name: string; description: string } | null
    roleplayTemplate?: { systemPrompt: string } | null
    toolInstructions?: string
    selectedSystemPromptId?: string | null
    timestampConfig?: TimestampConfig | null
    isInitialMessage?: boolean
    timezone?: string
    scenarioText?: string | null
    precompiledIdentityStack?: string | null
    tabooPhrases?: string[]
    standingInstructions?: string | null
  },
  nowMs: number,
) {
  PINNED_NOW = nowMs
  const localOffsetMin = new Date(nowMs).getTimezoneOffset()
  const out = buildSystemPrompt(opts)
  rows.push({
    kind: 'systemPrompt',
    id,
    character: opts.character,
    userCharacter: opts.userCharacter ?? null,
    roleplayTemplate: opts.roleplayTemplate ?? null,
    toolInstructions: opts.toolInstructions ?? null,
    selectedSystemPromptId: opts.selectedSystemPromptId ?? null,
    timestampConfig: opts.timestampConfig ?? null,
    isInitialMessage: opts.isInitialMessage ?? false,
    timezone: opts.timezone ?? null,
    scenarioText: opts.scenarioText ?? null,
    precompiledIdentityStack: opts.precompiledIdentityStack ?? null,
    // `undefined` (the option omitted) is emitted as null so the Rust side can
    // tell "no option" from "explicit empty list" — the two arms whose byte
    // identity keeps the golden prompt hash stable.
    tabooPhrases: opts.tabooPhrases ?? null,
    standingInstructions: opts.standingInstructions ?? null,
    nowMs,
    localOffsetMin,
    out,
  })
}

pushSystemPrompt('stack-only', { character: ch({ name: 'Ada', personality: 'Curious.' }) }, T_MAIN)
pushSystemPrompt(
  'with-roleplay',
  { character: ch({ name: 'Ada' }), roleplayTemplate: { systemPrompt: 'Format as {{char}} in prose.' } },
  T_MAIN,
)
pushSystemPrompt('empty-roleplay-skipped', { character: ch({ name: 'Ada' }), roleplayTemplate: { systemPrompt: '' } }, T_MAIN)
pushSystemPrompt(
  'with-tools',
  { character: ch({ name: 'Ada', pronouns: { subject: 'she', object: 'her', possessive: 'hers' } }), toolInstructions: 'Use the workspace tools when needed.' },
  T_MAIN,
)
// P4.D103 (v4 `346e855f`, bug 88): the reinforcement block no longer consults
// pronouns at all, so this row and `with-tools` above must emit the SAME final
// block. It used to be the row that produced "they CALLS them — they does not".
pushSystemPrompt(
  'with-tools-no-pronouns',
  { character: ch({ name: 'Ada' }), toolInstructions: 'Tool rules here.' },
  T_MAIN,
)
pushSystemPrompt(
  'roleplay-and-tools',
  {
    character: ch({ name: 'Ada', pronouns: { subject: 'he', object: 'him', possessive: 'his' } }),
    roleplayTemplate: { systemPrompt: 'Prose only.' },
    toolInstructions: 'Tools: search, edit.',
  },
  T_MAIN,
)
pushSystemPrompt(
  'precompiled-stack',
  { character: ch({ name: 'Ada', personality: 'ignored because precompiled' }), precompiledIdentityStack: '## Precompiled\nStable prefix for Ada.' },
  T_MAIN,
)
pushSystemPrompt(
  'precompiled-blank-falls-through',
  { character: ch({ name: 'Ada', personality: 'Used because precompiled is blank.' }), precompiledIdentityStack: '   \n  ' },
  T_MAIN,
)
// Timestamp path: EVERY_MESSAGE + autoPrepend:false → {{timestamp}} resolves.
pushSystemPrompt(
  'timestamp-injected-friendly',
  {
    character: ch({ name: 'Ada', personality: 'It is {{timestamp}} now.' }),
    timestampConfig: baseTsConfig({ mode: 'EVERY_MESSAGE', format: 'FRIENDLY', autoPrepend: false }),
    timezone: 'UTC',
  },
  T_MAIN,
)
pushSystemPrompt(
  'timestamp-injected-ny',
  {
    character: ch({ name: 'Ada', personality: 'Clock: {{timestamp}}' }),
    timestampConfig: baseTsConfig({ mode: 'EVERY_MESSAGE', format: 'ISO8601', autoPrepend: false }),
    timezone: 'America/New_York',
  },
  T_MAIN,
)
// autoPrepend true → timestamp NOT set (template var stays empty).
pushSystemPrompt(
  'timestamp-autoprepend-not-injected',
  {
    character: ch({ name: 'Ada', personality: 'Time is [{{timestamp}}]' }),
    timestampConfig: baseTsConfig({ mode: 'EVERY_MESSAGE', format: 'FRIENDLY', autoPrepend: true }),
    timezone: 'UTC',
  },
  T_MAIN,
)
// Mode NONE → shouldInject false → timestamp not set.
pushSystemPrompt(
  'timestamp-mode-none',
  {
    character: ch({ name: 'Ada', personality: 'Time is [{{timestamp}}]' }),
    timestampConfig: baseTsConfig({ mode: 'NONE', autoPrepend: false }),
    timezone: 'UTC',
  },
  T_MAIN,
)
// START_ONLY not initial → not injected.
pushSystemPrompt(
  'timestamp-start-only-not-initial',
  {
    character: ch({ name: 'Ada', personality: '[{{timestamp}}]' }),
    timestampConfig: baseTsConfig({ mode: 'START_ONLY', autoPrepend: false }),
    isInitialMessage: false,
    timezone: 'UTC',
  },
  T_MAIN,
)
// START_ONLY initial + tools + roleplay, custom format.
pushSystemPrompt(
  'timestamp-start-only-initial-custom',
  {
    character: ch({ name: 'Ada', pronouns: { subject: 'she', object: 'her', possessive: 'hers' } }),
    roleplayTemplate: { systemPrompt: 'RP at {{timestamp}}.' },
    toolInstructions: 'Tools ready.',
    timestampConfig: baseTsConfig({ mode: 'START_ONLY', format: 'CUSTOM', customFormat: 'YYYY-MM-DD HH:mm', autoPrepend: false }),
    isInitialMessage: true,
    timezone: 'UTC',
  },
  T_MAIN,
)

// ==== P4.D50: the Taboo section (v4 `7df7de8e`) ============================
// The section is rendered into the UNIVERSAL portion of system block 1, between
// the math note and the per-turn tool instructions. Placement is proven by the
// diffed bytes (not a substring assertion), so the two placement rows carry
// both a roleplay template and tool instructions.
const TABOO_CHAR = ch({
  name: 'Ada',
  personality: 'Curious.',
  pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
})
// Absent vs explicit-empty: these two MUST come out byte-identical — that is
// what keeps an untouched instance's cacheable prefix unchanged by the feature.
pushSystemPrompt('taboo-absent', { character: TABOO_CHAR }, T_MAIN)
pushSystemPrompt('taboo-empty-list', { character: TABOO_CHAR, tabooPhrases: [] }, T_MAIN)
pushSystemPrompt(
  'taboo-two-phrases-with-tools',
  {
    character: TABOO_CHAR,
    roleplayTemplate: { systemPrompt: 'Prose only, {{char}}.' },
    toolInstructions: 'Tools: search, edit.',
    tabooPhrases: ["that's not nothing", 'weight-bearing'],
  },
  T_MAIN,
)
pushSystemPrompt(
  'taboo-no-tools-after-math-note',
  { character: TABOO_CHAR, tabooPhrases: ['weight-bearing'] },
  T_MAIN,
)
pushSystemPrompt(
  'taboo-order-preserved',
  { character: TABOO_CHAR, tabooPhrases: ['zeta', 'alpha', 'mu'] },
  T_MAIN,
)
pushSystemPrompt(
  'taboo-blank-entries-dropped',
  { character: TABOO_CHAR, tabooPhrases: ['  weight-bearing  ', '   ', 'tapestry'] },
  T_MAIN,
)
// Every entry blank → no section at all (the renderer's second null arm).
pushSystemPrompt(
  'taboo-all-blank-omits-section',
  { character: TABOO_CHAR, tabooPhrases: ['', '   '] },
  T_MAIN,
)
// A phrase may legitimately contain template syntax; it must reach the model
// literally (the section never passes through processTemplate).
pushSystemPrompt(
  'taboo-template-syntax-literal',
  { character: TABOO_CHAR, tabooPhrases: ['as {{char}} always says'] },
  T_MAIN,
)
// Non-ASCII + a quote inside the phrase (the bullet wraps it in double quotes).
pushSystemPrompt(
  'taboo-nonascii-and-quotes',
  { character: TABOO_CHAR, tabooPhrases: ['un « je-ne-sais-quoi » 🎩', 'the "obvious" answer'] },
  T_MAIN,
)

// ==== P4.D103: the standing-instructions slot (v4 `8f868109`) ==============
// The section sits between the Taboo section and the tool instructions, and —
// unlike Taboo — IS template-processed.
const SI_CHAR = ch({
  name: 'Test Character',
  personality: 'Keeps a level head.',
  pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
})
const SI_SECTION = [
  '[STANDING INSTRUCTIONS]',
  "The sections below are standing instructions attached to this chat's project and to groups you belong to. They hold for the entire conversation. They refine how you conduct yourself here; they never replace who you are.",
  '',
  '## Project Instructions — Expedition',
  '{{char}} keeps the expedition journal, and {{user}} signs each entry.',
  '',
  '## Group Instructions — Cartographers',
  'No exclamation marks.',
].join('\n')

// Absent / empty / whitespace must all build the SAME prompt as the pre-feature
// layout — the Taboo byte-identity contract, one level up.
pushSystemPrompt('standing-absent', { character: SI_CHAR }, T_MAIN)
pushSystemPrompt('standing-empty-string', { character: SI_CHAR, standingInstructions: '' }, T_MAIN)
pushSystemPrompt(
  'standing-whitespace-only',
  { character: SI_CHAR, standingInstructions: '   \n\t ' },
  T_MAIN,
)
// Present, no tools: the section is the LAST block, after the math note.
pushSystemPrompt(
  'standing-present-no-tools',
  { character: SI_CHAR, standingInstructions: SI_SECTION },
  T_MAIN,
)
// Present with Taboo AND tools: the ORDER is math note → Taboo → standing
// instructions → tool instructions → reinforcement.
pushSystemPrompt(
  'standing-between-taboo-and-tools',
  {
    character: SI_CHAR,
    standingInstructions: SI_SECTION,
    tabooPhrases: ['weight-bearing'],
    toolInstructions: 'Tools: search, edit.',
  },
  T_MAIN,
)
// The template-processing discriminator: `{{char}}`/`{{user}}` MUST resolve
// (Taboo's phrases deliberately do not).
pushSystemPrompt(
  'standing-template-processed',
  {
    character: SI_CHAR,
    userCharacter: { name: 'Wren', description: 'a passing scholar' },
    standingInstructions: SI_SECTION,
  },
  T_MAIN,
)
// With a roleplay template in front of it too.
pushSystemPrompt(
  'standing-with-roleplay-and-tools',
  {
    character: SI_CHAR,
    roleplayTemplate: { systemPrompt: 'Prose only.' },
    standingInstructions: SI_SECTION,
    toolInstructions: 'Tools: search.',
  },
  T_MAIN,
)
// A section that is only leading/trailing whitespace around real content still
// renders — the guard is `.trim().length > 0`, not a trim of the pushed value.
pushSystemPrompt(
  'standing-untrimmed-pushed-verbatim',
  { character: SI_CHAR, standingInstructions: `\n\n${SI_SECTION}\n\n` },
  T_MAIN,
)

// ==== P4.D50: v4's cache-determinism golden fixture ========================
// The EXACT `FIXTURE_OPTIONS` / `FIXTURE_OPTIONS_WITH_TABOO` from v4's
// `__tests__/unit/cache-determinism/system-prompt.test.ts`. The Rust
// differential additionally hashes these two rows' output and asserts v4's
// checked-in golden constants — v5 inherits the golden through byte-identity
// rather than duplicating the fixture in Rust.
const GOLDEN_CHARACTER = ch({
  name: 'Iris Volney',
  description: 'A junior cartographer with a keen eye for geometric anomalies.',
  personality: 'Methodical, sceptical, and easily charmed by elegant proofs.',
  aliases: ['Vee', 'The Mapmaker'],
  pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
  systemPrompts: [
    {
      id: '22222222-2222-2222-2222-222222222222',
      name: 'default',
      content: 'You are a careful cartographer; you reason from triangulation, not flourish.',
      isDefault: true,
      createdAt: '2025-01-01T00:00:00.000Z',
      updatedAt: '2025-01-01T00:00:00.000Z',
    },
  ] as any,
  scenarios: [] as any,
  exampleDialogues: '"Two readings off — never one." Iris tapped the brass dial of her sextant.',
  physicalDescription: phys({
    name: 'standard',
    shortPrompt: 'auburn hair, ink-stained fingers, brass goggles around her neck',
    mediumPrompt: '',
    longPrompt: '',
    completePrompt: '',
    fullDescription: '',
    usageContext: 'general',
  }),
})
const GOLDEN_OPTS = {
  character: GOLDEN_CHARACTER,
  userCharacter: { name: 'Wren', description: 'a passing scholar' },
  roleplayTemplate: { systemPrompt: 'Respond in measured prose. Do not break role.' },
  toolInstructions: 'When invoking tools, do so via tool_use blocks; never narrate the call.',
  selectedSystemPromptId: '22222222-2222-2222-2222-222222222222',
  scenarioText: 'A draughty observatory two hours past midnight.',
  precompiledIdentityStack: null,
}
pushSystemPrompt('cache-golden-base', GOLDEN_OPTS, T_MAIN)
pushSystemPrompt(
  'cache-golden-with-taboo',
  { ...GOLDEN_OPTS, tabooPhrases: ["that's not nothing", 'weight-bearing'] },
  T_MAIN,
)

// ==== buildOtherParticipantsInfo ===========================================
function participant(overrides: Partial<ChatParticipantBase> & { id: string; characterId: string }): ChatParticipantBase {
  return {
    id: overrides.id,
    type: 'CHARACTER',
    characterId: overrides.characterId,
    controlledBy: 'llm',
    displayOrder: 0,
    isActive: true,
    status: 'active',
    hasHistoryAccess: false,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...overrides,
  } as ChatParticipantBase
}

function pushOthers(id: string, respondingParticipantId: string, participants: ChatParticipantBase[], characters: Record<string, Character>) {
  const map = new Map(Object.entries(characters))
  const out = buildOtherParticipantsInfo(respondingParticipantId, participants, map)
  rows.push({ kind: 'otherParticipants', id, respondingParticipantId, participants, characters, out })
}

{
  const ada = ch({ name: 'Ada', aliases: ['Cog'], pronouns: { subject: 'she', object: 'her', possessive: 'hers' }, title: 'the rival', description: 'gears' })
  const bram = ch({ name: 'Bram', description: 'a brooding sort' })
  const cleo = ch({ name: 'Cleo' })

  // 0 others (only self).
  pushOthers('zero-others', 'p1', [participant({ id: 'p1', characterId: 'ada' })], { ada })

  // 1 other, uses title over description.
  pushOthers(
    'one-other-title-over-desc',
    'p1',
    [participant({ id: 'p1', characterId: 'ada' }), participant({ id: 'p2', characterId: 'bram' })],
    { ada, bram },
  )

  // Many others, mix of statuses; a removed one is skipped; description fallback; no title/desc → undefined.
  pushOthers(
    'many-others-mixed',
    'p1',
    [
      participant({ id: 'p1', characterId: 'ada' }),
      participant({ id: 'p2', characterId: 'bram', status: 'silent' }),
      participant({ id: 'p3', characterId: 'cleo', status: 'active' }),
      participant({ id: 'p4', characterId: 'bram', status: 'removed' }),
    ],
    { ada, bram, cleo },
  )

  // Missing character in the map → participant skipped.
  pushOthers(
    'missing-character-skipped',
    'p1',
    [participant({ id: 'p1', characterId: 'ada' }), participant({ id: 'p2', characterId: 'ghost' })],
    { ada },
  )

  // Absent status is retained (not removed).
  pushOthers(
    'absent-retained',
    'p1',
    [participant({ id: 'p1', characterId: 'ada' }), participant({ id: 'p2', characterId: 'cleo', status: 'absent' })],
    { ada, cleo },
  )

  // Nonascii character info.
  const elise = ch({ name: 'Élise', aliases: ['Élisa'], pronouns: { subject: 'elle', object: 'la', possessive: 'sa' }, description: 'une voyageuse 🎩' })
  pushOthers(
    'nonascii-other',
    'p1',
    [participant({ id: 'p1', characterId: 'ada' }), participant({ id: 'p2', characterId: 'elise' })],
    { ada, elise },
  )
}

// ==== buildIdentityReinforcement ===========================================
for (const name of ['Ada', 'Élise 🎩', 'Bram "the Bold"']) {
  rows.push({ kind: 'reinforcement', id: `reinforce-${name}`, characterName: name, out: buildIdentityReinforcement(name) })
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
