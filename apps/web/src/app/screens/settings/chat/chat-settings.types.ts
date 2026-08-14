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
// Dangerous content (v4 lib/schemas/settings.types.ts L302-325 +
// DangerousContentSettings.tsx L18-58)
// ---------------------------------------------------------------------------

export type DangerousContentMode = 'OFF' | 'DETECT_ONLY' | 'AUTO_ROUTE';
export type DangerousContentDisplayMode = 'SHOW' | 'BLUR' | 'COLLAPSE';

export interface DangerousContentSettings {
  mode: DangerousContentMode;
  threshold: number;
  scanTextChat: boolean;
  scanImagePrompts: boolean;
  scanImageGeneration: boolean;
  uncensoredTextProfileId?: string | null;
  uncensoredImageProfileId?: string | null;
  displayMode: DangerousContentDisplayMode;
  showWarningBadges: boolean;
  customClassificationPrompt?: string | null;
}

export const DANGEROUS_MODE_OPTIONS: readonly {
  value: DangerousContentMode;
  label: string;
  description: string;
}[] = [
  { value: 'OFF', label: 'Off', description: 'No content scanning or routing' },
  {
    value: 'DETECT_ONLY',
    label: 'Detect Only',
    description: 'Scan and flag content, but do not reroute to uncensored providers',
  },
  {
    value: 'AUTO_ROUTE',
    label: 'Auto-Route',
    description:
      'Scan content and automatically route flagged messages to uncensored-compatible providers',
  },
];

export const DANGEROUS_DISPLAY_MODE_OPTIONS: readonly {
  value: DangerousContentDisplayMode;
  label: string;
  description: string;
}[] = [
  { value: 'SHOW', label: 'Show', description: 'Display flagged content normally with a warning badge' },
  { value: 'BLUR', label: 'Blur', description: 'Blur flagged content until clicked to reveal' },
  {
    value: 'COLLAPSE',
    label: 'Collapse',
    description: 'Collapse flagged content behind a placeholder',
  },
];

/**
 * v4 keeps a card-local `DEFAULT_SETTINGS` in `DangerousContentSettings.tsx`
 * (L52-60) that is byte-identical to `types.ts`'s
 * `DEFAULT_DANGEROUS_CONTENT_SETTINGS` (L495-503) — the duplication is v4's, and
 * collapsing it here is safe because the two agree.
 */
export const DEFAULT_DANGEROUS_CONTENT_SETTINGS: DangerousContentSettings = {
  mode: 'OFF',
  threshold: 0.7,
  scanTextChat: true,
  scanImagePrompts: true,
  scanImageGeneration: false,
  displayMode: 'SHOW',
  showWarningBadges: true,
};

// ---------------------------------------------------------------------------
// Cheap LLM (v4 types.ts L29-37) — the Dangerous Content card hosts the
// image-prompt-expansion picker, which writes into this bag.
// ---------------------------------------------------------------------------

export interface CheapLLMSettings {
  strategy: 'USER_DEFINED' | 'PROVIDER_CHEAPEST' | 'LOCAL_FIRST';
  userDefinedProfileId?: string | null;
  defaultCheapProfileId?: string | null;
  fallbackToLocal: boolean;
  embeddingProvider: 'SAME_PROVIDER' | 'OPENAI' | 'LOCAL';
  /** Optional override for image prompt expansion LLM — when set, uses this instead of global cheap LLM */
  imagePromptProfileId?: string | null;
}

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
