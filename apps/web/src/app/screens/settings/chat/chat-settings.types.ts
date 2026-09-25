/**
 * The Chat-tab card types + option tables + defaults, ported from v4
 * `components/settings/chat-settings/types.ts`. The option tables carry v4's
 * labels/descriptions verbatim — they ARE the user-facing copy, so they live
 * beside the cards that render them rather than being re-typed per card.
 *
 * Only the slices the eleven P4.6an cards need are ported; the rest of v4's
 * `types.ts` (avatar modes, timestamp config, embedding profiles) belongs to
 * the Appearance/Memory tabs and is ported where those live.
 *
 * @module screens/settings/chat/chat-settings.types
 */

import type {
  ConciergeDisplaySettingsDto,
  ConciergePreScreenSettingsDto,
  ConciergeSettingsDto,
} from '../../../core/core-contract';

// ---------------------------------------------------------------------------
// Token display (v4 types.ts L20-27, L360-367, L391-414)
// ---------------------------------------------------------------------------

export interface TokenDisplaySettings {
  showPerMessageTokens: boolean;
  showPerMessageCost: boolean;
  showChatTotals: boolean;
  showSystemEvents: boolean;
}

export const DEFAULT_TOKEN_DISPLAY_SETTINGS: TokenDisplaySettings = {
  showPerMessageTokens: false,
  showPerMessageCost: false,
  showChatTotals: false,
  showSystemEvents: false,
};

export const TOKEN_DISPLAY_OPTIONS: readonly {
  key: keyof TokenDisplaySettings;
  label: string;
  description: string;
}[] = [
  {
    key: 'showPerMessageTokens',
    label: 'Show Token Count on Messages',
    description: 'Display the number of prompt and completion tokens for each message',
  },
  {
    key: 'showPerMessageCost',
    label: 'Show Cost Estimate on Messages',
    description: 'Display estimated cost for each message (requires pricing data)',
  },
  {
    key: 'showChatTotals',
    label: 'Show Chat Token Totals',
    description: 'Display aggregate token counts and cost at the top of the chat',
  },
  {
    key: 'showSystemEvents',
    label: 'Show System Events in Chat',
    description:
      'Display background LLM operations (memory extraction, summarization, etc.) in the chat timeline',
  },
];

// ---------------------------------------------------------------------------
// Memory cascade (v4 types.ts L14, L54-57, L316-352)
// ---------------------------------------------------------------------------

export type MemoryCascadeAction =
  | 'DELETE_MEMORIES'
  | 'KEEP_MEMORIES'
  | 'REGENERATE_MEMORIES'
  | 'ASK_EVERY_TIME';

export interface MemoryCascadePreferences {
  onMessageDelete: MemoryCascadeAction;
  onSwipeRegenerate: MemoryCascadeAction;
}

export const MEMORY_CASCADE_ACTIONS: readonly {
  value: MemoryCascadeAction;
  label: string;
  description: string;
}[] = [
  {
    value: 'ASK_EVERY_TIME',
    label: 'Ask every time',
    description: 'Show a confirmation dialog to choose what to do',
  },
  {
    value: 'DELETE_MEMORIES',
    label: 'Always delete memories',
    description: 'Automatically delete associated memories',
  },
  {
    value: 'KEEP_MEMORIES',
    label: 'Always keep memories',
    description: 'Keep memories (they will become orphaned)',
  },
  {
    value: 'REGENERATE_MEMORIES',
    label: 'Delete and regenerate',
    description: 'Delete old memories and extract new ones from context',
  },
];

export const DEFAULT_MEMORY_CASCADE_PREFERENCES: MemoryCascadePreferences = {
  onMessageDelete: 'ASK_EVERY_TIME',
  onSwipeRegenerate: 'DELETE_MEMORIES',
};

// ---------------------------------------------------------------------------
// Context compression (v4 types.ts L369-389)
// ---------------------------------------------------------------------------

export interface ContextCompressionSettings {
  enabled: boolean;
  windowSize: number;
  compressionTargetTokens: number;
  systemPromptTargetTokens: number;
  /** How often to re-inject project context (0 = never after initial, must be >= windowSize) */
  projectContextReinjectInterval: number;
}

export const DEFAULT_CONTEXT_COMPRESSION_SETTINGS: ContextCompressionSettings = {
  enabled: true,
  windowSize: 5,
  compressionTargetTokens: 800,
  systemPromptTargetTokens: 1500,
  projectContextReinjectInterval: 5,
};

// ---------------------------------------------------------------------------
// Automation (v4 types.ts L426-441)
// ---------------------------------------------------------------------------

export const AUTOMATION_OPTIONS: readonly {
  key: 'autoDetectRng';
  label: string;
  description: string;
}[] = [
  {
    key: 'autoDetectRng',
    label: 'Auto-Detect RNG Calls',
    description:
      'Automatically detect dice rolls (e.g., 2d6), coin flips, and "spin the bottle" in your messages and execute them',
  },
];

export const DEFAULT_AUTO_DETECT_RNG = true;

// ---------------------------------------------------------------------------
// Agent mode (v4 types.ts L443-459, MAX_TURNS_OPTIONS from AgentModeSettings.tsx L14-20)
// ---------------------------------------------------------------------------

export interface AgentModeSettings {
  /** Maximum number of agent turns (1-25) */
  maxTurns: number;
  /** Whether agent mode is enabled by default for new chats */
  defaultEnabled: boolean;
}

export const DEFAULT_AGENT_MODE_SETTINGS: AgentModeSettings = {
  maxTurns: 10,
  defaultEnabled: false,
};

export const MAX_TURNS_OPTIONS: readonly { value: number; label: string }[] = [
  { value: 5, label: '5 turns' },
  { value: 10, label: '10 turns (default)' },
  { value: 15, label: '15 turns' },
  { value: 20, label: '20 turns' },
  { value: 25, label: '25 turns (maximum)' },
];

// ---------------------------------------------------------------------------
// Thinking / reasoning display (v4 types.ts L104-118) — DISPLAY ONLY
// ---------------------------------------------------------------------------

export interface ThinkingDisplaySettings {
  /** Whether new chats show captured thinking by default. */
  defaultVisible?: boolean;
  /** Whether the thinking block starts collapsed when shown. */
  defaultCollapsed?: boolean;
}

export const DEFAULT_THINKING_DISPLAY_SETTINGS: ThinkingDisplaySettings = {
  defaultVisible: true,
  defaultCollapsed: true,
};

// ---------------------------------------------------------------------------
// Answer confirmation (v4 types.ts L120-134)
// ---------------------------------------------------------------------------

export interface AnswerConfirmationSettings {
  /** Whether the consistency check runs by default (global). */
  enabled?: boolean;
}

export const DEFAULT_ANSWER_CONFIRMATION_SETTINGS: AnswerConfirmationSettings = {
  enabled: false,
};

// ---------------------------------------------------------------------------
// The Concierge (v4 `3b463d6b1` types.ts L529-548 + `lib/services/
// dangerous-content/resolver.service.ts` `DEFAULT_CONCIERGE_SETTINGS`). Replaces
// the retired Dangerous Content bag, the Image Description card's uncensored
// fallback scalar and the cheap-LLM bag's image-prompt override — all three
// now live inside `conciergeSettings`, and the server refuses a PUT that still
// carries any of them.
// ---------------------------------------------------------------------------

/** v4 `ConciergeSettings` — single-sourced from the wire shape, as v4 single-sources it from the Zod schema. */
export type ConciergeSettings = ConciergeSettingsDto;
export type ConciergeDisplaySettings = ConciergeDisplaySettingsDto;
export type ConciergePreScreenSettings = ConciergePreScreenSettingsDto;

/**
 * v4 `ConciergeSettingsUpdate`: a partial Concierge update — top-level fields
 * replace, `display` and `preScreen` deep-merge (see `mergeConciergeUpdate`).
 */
export type ConciergeSettingsUpdate = Partial<Omit<ConciergeSettings, 'display' | 'preScreen'>> & {
  display?: Partial<ConciergeDisplaySettings>;
  preScreen?: Partial<ConciergePreScreenSettings>;
};

/**
 * v4 `DEFAULT_CONCIERGE_SETTINGS` — the server's defaults (every profile id and
 * the custom prompt `null`, the schema defaults otherwise). v4 imports the
 * server's object into its client; v5's client and server are separate
 * programs, so this is a transcription, pinned field by field in
 * `concierge-settings.api.spec.ts`.
 */
export const DEFAULT_CONCIERGE_SETTINGS: ConciergeSettings = {
  enabled: true,
  uncensoredTextProfileId: null,
  uncensoredImageProfileId: null,
  uncensoredVisionProfileId: null,
  imagePromptProfileId: null,
  autoSwitchAfterRefusals: 2,
  newChatsStartAs: 'moderated',
  display: {
    mode: 'SHOW',
    showWarningBadges: true,
  },
  preScreen: {
    enabled: false,
    threshold: 0.7,
    scanTextChat: true,
    scanImagePrompts: true,
    scanImageGeneration: false,
    customClassificationPrompt: null,
    summaryClassification: false,
  },
};

// ---------------------------------------------------------------------------
// Smart typography (v4 types.ts L150-169, `2d31810f`) — Layer 1.6's one bag:
// Part A's render-time quote curling and Part B's type-time substitutions,
// stored together because they are one feature to the person using them.
// ---------------------------------------------------------------------------

export interface SmartTypographySettings {
  /** Curl quotes when displaying messages. Stored text is never modified. */
  displayQuotes?: boolean;
  /** `--` → en dash and `---` → em dash, as you type. */
  dashes?: boolean;
  /** `...` → ellipsis, as you type. */
  ellipsis?: boolean;
}

export const DEFAULT_SMART_TYPOGRAPHY_SETTINGS: SmartTypographySettings = {
  displayQuotes: false,
  dashes: true,
  ellipsis: true,
};
