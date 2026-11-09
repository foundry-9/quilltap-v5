/**
 * Oracle case (P4.D370, v4 `96156566b` at `624d0d25c`): Agent Mode flavours —
 * the cascade decides whether Agent Mode is on, the responding character's kind
 * decides what it means.
 *
 * Drives the REAL functions from v4's
 * lib/services/chat-message/agent-mode-resolver.service.ts:
 * resolveAgentModeSetting (the cascade + the flavour + its
 * `[AgentMode] Resolved agent mode` INFO), buildAgentModeInstructions and
 * buildForceFinalMessage. Strings are compared EXACTLY (each is a prompt
 * block); the INFO is captured off the REAL `Logger` prototype (singleton and
 * children alike, before the level check) — v4's own test mocks
 * `create-logger`, which is NOT copied here.
 *
 * Row kinds:
 *   - `resolve`: a planted chat / project / character / globalSettings; out =
 *     the ResolvedAgentMode + the captured INFO lines (message + context, v4's
 *     key order — `chatId` is absent when the chat is null).
 *   - `instructions`: maxTurns × both flavours.
 *   - `force_final`: both flavours.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/agent-mode-flavor.ts \
 *     > /tmp/oracle-agent-mode-flavor.ndjson
 */

type Captured = { message: string; context: unknown };

async function main(): Promise<void> {
  const captured: Captured[] = [];
  {
    const { Logger } = await import('@/lib/logger');
    const original = Logger.prototype.info;
    Logger.prototype.info = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (message === '[AgentMode] Resolved agent mode') {
        // JSON round trip: v4's `chatId: chat?.id` is `undefined` for a null
        // chat, which the file layer (and this record) omits.
        captured.push({ message, context: JSON.parse(JSON.stringify(context ?? {})) });
        // Not forwarded: the console transport would interleave the line with
        // this case's NDJSON on stdout.
        return undefined;
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  const { resolveAgentModeSetting, buildAgentModeInstructions, buildForceFinalMessage } = await import(
    '@/lib/services/chat-message/agent-mode-resolver.service'
  );

  const rows: unknown[] = [];

  // ---- resolve: the character × cascade grid ----
  const characters: Array<[string, Record<string, unknown> | null]> = [
    ['roleplay', { kind: 'roleplay' }],
    ['pre-kind', {}],
    ['null-character', null],
    ['assistant', { kind: 'assistant' }],
    ['robot', { kind: 'robot' }],
    ['capital-assistant', { kind: 'Assistant' }],
    ['kind-null', { kind: null }],
    ['kind-five', { kind: 5 }],
  ];
  type Cascade = {
    global: Record<string, unknown> | null;
    characterDefault?: unknown;
    project?: Record<string, unknown> | null;
    chatEnabled?: boolean | null;
  };
  const ON = { agentModeSettings: { maxTurns: 7, defaultEnabled: true } };
  const OFF = { agentModeSettings: { maxTurns: 12, defaultEnabled: false } };
  const cascades: Array<[string, Cascade]> = [
    ['global-on', { global: ON }],
    ['global-off', { global: OFF }],
    ['character-true', { global: OFF, characterDefault: true }],
    ['character-false', { global: ON, characterDefault: false }],
    // A non-boolean stored default: v4 assigns it as is (`!== null && !==
    // undefined`); v5 reads it as unset — the named divergence
    // NON_BOOLEAN_CHARACTER_AGENT_DEFAULT in the Rust test.
    ['character-yes', { global: OFF, characterDefault: 'yes' }],
    ['project-override', { global: ON, characterDefault: true, project: { defaultAgentModeEnabled: false } }],
    ['chat-override', { global: OFF, characterDefault: false, project: { defaultAgentModeEnabled: false }, chatEnabled: true }],
    ['chat-null-inherits', { global: ON, chatEnabled: null, project: { defaultAgentModeEnabled: null } }],
  ];

  for (const [charId, base] of characters) {
    for (const [cascadeId, c] of cascades) {
      if (base === null && c.characterDefault !== undefined) continue; // no row to carry it
      const character =
        base === null
          ? null
          : c.characterDefault === undefined
            ? base
            : { ...base, defaultAgentModeEnabled: c.characterDefault };
      const chat: Record<string, unknown> = { id: `chat-${charId}-${cascadeId}` };
      if (c.chatEnabled !== undefined) chat.agentModeEnabled = c.chatEnabled;
      const project = c.project ?? null;
      captured.length = 0;
      const out = resolveAgentModeSetting(chat as never, project as never, character as never, c.global as never);
      rows.push({
        kind: 'resolve',
        id: `${charId}--${cascadeId}`,
        chat,
        project,
        character,
        globalSettings: c.global,
        out,
        lines: [...captured],
      });
    }
  }
  // The `chat: null` arm (`chatId` OMITTED) — reachable only from this driver;
  // every v5 caller holds the chat. And a null globalSettings (the
  // DEFAULT_AGENT_MODE_SETTINGS fallback).
  for (const [id, chat, character, globalSettings] of [
    ['null-chat--assistant', null, { kind: 'assistant' }, ON],
    ['null-chat--roleplay-default-on', null, { kind: 'roleplay', defaultAgentModeEnabled: true }, OFF],
    ['null-settings--assistant', { id: 'chat-null-settings' }, { kind: 'assistant' }, null],
  ] as Array<[string, Record<string, unknown> | null, Record<string, unknown>, Record<string, unknown> | null]>) {
    captured.length = 0;
    const out = resolveAgentModeSetting(chat as never, null, character as never, globalSettings as never);
    rows.push({ kind: 'resolve', id, chat, project: null, character, globalSettings, out, lines: [...captured] });
  }

  // ---- instructions / force_final ----
  for (const flavor of ['research', 'in-character'] as const) {
    for (const maxTurns of [1, 6, 25, 100]) {
      rows.push({ kind: 'instructions', id: `${flavor}-${maxTurns}`, flavor, maxTurns, out: buildAgentModeInstructions(maxTurns, flavor) });
    }
    rows.push({ kind: 'force_final', id: flavor, flavor, out: buildForceFinalMessage(flavor) });
  }

  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
}

main().catch((e) => {
  process.stderr.write(String(e?.stack ?? e) + '\n');
  process.exit(1);
});
