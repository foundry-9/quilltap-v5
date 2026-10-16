/**
 * P4.D305 (v4 `362426c8f`) — the trigger-phrase room of the committed
 * `pascal-run-custom-{main,mount}.db` pair, shared by the full builder
 * (`build-pascal-run-custom-fixture.ts`) and the in-place widen
 * (`extend-pascal-run-custom-triggers.ts`) that put it into the committed pair.
 *
 * Everything goes through v4's REAL repositories and `storeMountFile`; nothing
 * here is a hand-written row.
 */

const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const CHAR_B = 'a1000000-0000-4000-8000-00000000000b';

/**
 * P4.D305 (v4 `362426c8f`): the TRIGGER-PHRASE room. Its own project carries a
 * linked document store — the "Trigger Shelf" — holding every trigger-bearing
 * tool, so the tools reach this room ONLY (the project tier) and no existing
 * room's roster moves. CHAR_A plays an LLM seat and CHAR_B a USER-driven seat
 * (a seat that speaks AS its character), plus the bare operator's seat.
 * Tool names are chosen so no CHAR_A / CHAR_B vault tool shadows them (a
 * character-tier `oracle` would, which is why the sheet-gated tool is `seer`).
 */
export const CHAT_TRIGGERS = 'c1000000-0000-4000-8000-000000000007';
export const PROJECT_TRIGGERS = 'a3000000-0000-4000-8000-0000000000bb';
export const SHELF_MP = '93000000-0000-4000-8000-0000000000bb';
export const P7_A = 'e7000000-0000-4000-8000-00000000000a';
export const P7_B = 'e7000000-0000-4000-8000-00000000000b';

/** One state write, so a triggered deal's effects are on the record. */
const SPELL = {
  name: 'spell',
  title: 'Cast A Spell',
  description: 'Say the words and the spell is cast.',
  triggers: ['abracadabra', 'open sesame'],
  roll: { min: 0.8, max: 0.8 },
  effects: [{ target: 'state.spells.cast', value: 1 }],
  outcomes: [{ when: true, message: 'The air shimmers.', state: 'success' }],
};

/** Sheet-gated: CHAR_A's `blessed: true` is offered it; CHAR_B is not. */
const SEER = {
  name: 'seer',
  description: 'Consult the seer, if you are blessed.',
  triggers: ['consult the oracle'],
  availableWhen: { metadata: { blessed: { eq: true } } },
  roll: { min: 0.6, max: 0.6 },
  outcomes: [{ when: true, message: 'The seer nods.', state: 'info' }],
};

/** Every parameter carries a default (the format requires one), so a
 *  triggered run — every parameter at its default — always binds. What
 *  REFUSES this one is the deal: its `$param`-bound roll's DEFAULT bounds are
 *  inverted (low above high), which only the run can see, so
 *  `executeCustomTool` throws and Prospero reports it. */
const INITIATIVE = {
  name: 'initiative',
  description: 'Roll initiative between two bounds.',
  triggers: ['draw my sword'],
  parameters: {
    low: { type: 'number', default: 0.9, description: 'The low bound.' },
    high: { type: 'number', default: 0.1, description: 'The high bound.' },
  },
  roll: { min: { $param: 'low' }, max: { $param: 'high' } },
  outcomes: [{ when: true, message: 'You strike.', state: 'success' }],
};

/** Whispered by default: a triggered run is targeted to the SPEAKER. */
const HUSH = {
  name: 'hush',
  description: 'A quiet word.',
  triggers: ['keep it quiet'],
  defaultVisibility: 'whisper',
  roll: { min: 0.5, max: 0.5 },
  outcomes: [{ when: true, message: 'Only you hear it.', state: 'info' }],
};

/** Five one-word tools — one more than the per-message cap. */
const CAP_TOOLS = [0, 1, 2, 3, 4].map((i) => ({
  name: `t${i}`,
  description: `Cap probe ${i}.`,
  triggers: [`xyzzy${i}`],
  roll: { min: 0.5, max: 0.5 },
  outcomes: [{ when: true, message: `Cap probe ${i} fires.`, state: 'info' }],
}));

/** The single-clock pair: each WRITES a progression, so both stamp the ONE
 *  `nowMs` the runner read for the whole message. */
const WIND = {
  name: 'wind',
  description: 'Wind the clock.',
  triggers: ['wind the clock'],
  roll: { min: 0.5, max: 0.5 },
  effects: [{ target: 'progress.clockwork.endTime', value: '{{now}} + 600000' }],
  outcomes: [{ when: true, message: 'Tick.', state: 'success' }],
};
const STOKE = {
  name: 'stoke',
  description: 'Stoke the fire.',
  triggers: ['stoke the fire'],
  roll: { min: 0.5, max: 0.5 },
  effects: [{ target: 'progress.hearth.endTime', value: '{{now}} + 300000' }],
  outcomes: [{ when: true, message: 'The fire roars.', state: 'success' }],
};

/** Disabled: never in the roster, so its phrase fires nothing. */
const DORMANT = {
  name: 'dormant',
  description: 'Asleep.',
  disabled: true,
  triggers: ['sleeping words'],
  roll: { min: 0.5, max: 0.5 },
  outcomes: [{ when: true, message: 'It stirs.', state: 'info' }],
};

/** Consult-bearing: the runner builds an invoker only when the definition
 *  declares `llm`, and this fixture carries no connection profiles. */
export const AUGUR = {
  name: 'augur',
  description: 'Read the omens.',
  triggers: ['read the omens'],
  roll: { min: 0.7, max: 0.7 },
  llm: {
    prompt: 'The draw was {{value}}. Answer YES or NO: are the omens good?',
    errorMessage: 'The omens are clouded.',
    maxOutput: 120,
  },
  outcomes: [
    { when: { llm: { ok: true, eq: 'YES' } }, message: 'The omens say {{llm}}.', state: 'success' },
    { when: { llm: { ok: false } }, message: 'Clouded: {{llm}}', state: 'failure' },
    { when: true, message: 'The augur shrugs.', state: 'info' },
  ],
};


/** Every tool on the shelf, in the order they are written. */
export const TRIGGER_SHELF_TOOLS = [SPELL, SEER, INITIATIVE, HUSH, ...CAP_TOOLS, WIND, STOKE, DORMANT, AUGUR];

export interface TriggerRoomContext {
  // v4's repository container (`getRepositories()`).
  repos: any; // eslint-disable-line @typescript-eslint/no-explicit-any
  writeVaultFile: (mountPointId: string, relativePath: string, body: unknown) => Promise<void>;
  userId: string;
  ts: string;
  mkParticipant: (id: string, characterId: string, controlledBy: string) => Record<string, unknown>;
}

/** The project, its linked Trigger Shelf store, the shelf's tools, and the
 *  room. Like the perspective rooms, the room carries NO messages. */
export async function addTriggerRoom(ctx: TriggerRoomContext): Promise<void> {
  const { repos, writeVaultFile, userId, ts, mkParticipant } = ctx;
  const project = await repos.projects.create(
    { name: 'The Trigger Shelf', description: null, state: {} },
    { id: PROJECT_TRIGGERS, createdAt: ts, updatedAt: ts },
  );
  if ((project as { id: string }).id !== PROJECT_TRIGGERS) throw new Error('trigger project id drift');
  await repos.docMountPoints.create(
    {
      name: 'Trigger Shelf',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    },
    { id: SHELF_MP, createdAt: ts, updatedAt: ts },
  );
  await repos.projectDocMountLinks.link(PROJECT_TRIGGERS, SHELF_MP);
  for (const tool of TRIGGER_SHELF_TOOLS) {
    await writeVaultFile(SHELF_MP, `Tools/${tool.name}.tool.json`, tool);
  }
  await repos.chats.create(
    {
      userId,
      title: 'Pascal — the trigger room',
      chatType: 'salon',
      contextSummary: null,
      participants: [mkParticipant(P7_A, CHAR_A, 'llm'), mkParticipant(P7_B, CHAR_B, 'user')],
      projectId: PROJECT_TRIGGERS,
      tags: [],
    },
    { id: CHAT_TRIGGERS, createdAt: ts, updatedAt: ts },
  );
  await repos.chats.update(CHAT_TRIGGERS, { updatedAt: ts });
}
