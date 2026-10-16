/**
 * Composer slash commands — types (pure logic).
 *
 * A character-for-character transcription of v4 `lib/slash-commands/types.ts`
 * at `25b8fab6a` (added by `4877fc2da`). v4's header, carried:
 *
 * ⚠ PORTABILITY: every file in `lib/slash-commands/` imports only from
 * `lib/char-insert/` (by relative path) and its own siblings — no React, no
 * Lexical, no Next, no TanStack, no `@/lib/*` helper. quilltap-v5 copies this
 * directory the way it copies `lib/char-insert/`. (v4 enforces that with an
 * ESLint `no-restricted-imports` override; `apps/web` has no ESLint, so here
 * the rule is kept by convention — `chat/slash-commands/` imports only
 * `editor/char-insert/{trigger,types}` and its siblings — and the transcription
 * is proven row for row by `slash-commands.oracle.spec.ts`, a corpus recorded
 * from v4's REAL module.) The ProseMirror adapter lives in
 * `editor/slash/slash-command-plugin.ts` (v4: `components/chat/lexical/plugins/
 * SlashCommandPlugin.tsx`).
 *
 * Design of record (v4): `docs/developer/features/complete/composer-slash-commands.md`.
 *
 * @module chat/slash-commands/types
 */

export type SlashCommandKind = 'action' | 'builtin-tool' | 'custom-tool';

/**
 * The composer's own actions — the gutter buttons, the two tool dialogs with
 * nothing pre-selected, and the dice button's quick rolls.
 */
export type ComposerAction =
  | 'attach'
  | 'library'
  | 'image'
  | 'announce'
  | 'mail'
  | 'inform'
  /** The Run Tool dialog, no pre-selection. */
  | 'tool'
  /** The custom-tool dialog, no pre-selection. */
  | 'pascal'
  /** The Run Tool dialog, pre-selected on `rng`. */
  | 'roll'
  | 'd6'
  | 'd20'
  | 'coin'
  | 'bottle';

/** What the adapter dispatches on when a command is picked. */
export type SlashCommandPayload =
  | { kind: 'action'; action: ComposerAction }
  | { kind: 'builtin-tool'; toolId: string }
  | { kind: 'custom-tool'; name: string; asCharacterId: string | null };

export interface SlashCommand {
  /** Stable identity: 'action:attach', 'builtin:wardrobe_wear', 'custom:<name>::<asCharacterId>'. */
  key: string;
  kind: SlashCommandKind;
  /** The word after the slash: 'attach', 'wardrobe_wear', 'unlock'. Matched case-insensitively. */
  command: string;
  /** Human label: 'Attach file', 'Wear Wardrobe Items', 'Force the Lock'. */
  label: string;
  /** One line, shown muted. Tools use their description. */
  description: string;
  /** Extra match terms: the tool's category, a custom tool's character label. */
  keywords: string[];
  /** Muted trailing detail: 'Composer', 'Prospero · wardrobe', 'Pascal', 'Pascal · as Vivienne'. */
  source: string;
  /**
   * Whether picking it opens a dialog to complete. Tools: declares at least one
   * parameter. Actions: whether the action is one of the tool dialogs (their
   * own dialogs — image, announce, mail, inform — are the action itself).
   */
  needsDialog: boolean;
  payload: SlashCommandPayload;
}

/** A built-in (Prospero) tool as `GET /api/v1/tools?chatId=…&includeSchemas=true` lists it. */
export interface SlashBuiltInToolSource {
  id: string;
  name: string;
  description: string;
  category?: string;
  pluginName?: string;
  available?: boolean;
  unavailableReason?: string;
  userInvocable?: boolean;
  parameters?: { properties?: Record<string, unknown> } | Record<string, unknown>;
}

/** A Pascal custom tool as the chat's custom-tool roster lists it. */
export interface SlashCustomToolSource {
  name: string;
  title: string;
  description: string;
  parameters: Record<string, unknown>;
  characterLabel?: string;
  asCharacterId?: string | null;
}

export interface SlashCommandSources {
  /** Which composer actions are wired in this composer. */
  actions: ReadonlySet<ComposerAction>;
  builtInTools: readonly SlashBuiltInToolSource[];
  customTools: readonly SlashCustomToolSource[];
}
