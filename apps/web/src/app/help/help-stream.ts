/**
 * The help-chat stream fold — a port of v4
 * `components/help-chat/hooks/useHelpChatStreaming.ts` (baseline `d883a5ee1`).
 *
 * v4's hook is a small, self-contained state machine over the SSE frames, and
 * deliberately NOT the Salon's: it keeps one content buffer, one participant,
 * and two link collections, and it CLEARS the buffer on `status` so a tool pass
 * shows a "working" line instead of stale prose. So this is a transcription of
 * that machine rather than a reuse of `chat/chat-stream.reducer.ts` — the two
 * disagree on exactly that point, and the help surface wants v4's answer.
 *
 * Split pure (here) from transport ({@link HelpStreamingService}) so the whole
 * machine is unit-testable against v4's own cases without a fake event stream.
 *
 * Two recorded shape divergences, both forced by v5's flat frame envelope (§B:
 * the frames ride the global Event channel exactly like `ChatSend`, so they
 * arrive in the SAME shape as every other chat frame, not v4's SSE JSON):
 *
 *  - v4 reads `event.turnStart.participantId` / `event.status.participantId`
 *    from NESTED objects; v5's frame carries `participantId` as a SIBLING of
 *    the `turnStart` / `status` markers, so those reads move to the sibling.
 *  - v4 reads the participant off `event.status.participantId`; v5's
 *    `ResponseStatus` carries `characterId`, and §B routes the participant
 *    through the envelope's sibling `participantId` instead. Only the status
 *    frame's PRESENCE is otherwise load-bearing here.
 *
 * One recorded BEHAVIOURAL divergence, deliberate: v4 records `event.error`
 * bare, so v5 does too — NOT the shared reducer's `${error}: ${details}` join.
 * §B pins `details: ''` on the help error frame, so the join would only ever
 * append a colon.
 *
 * @module help/help-stream
 */

import type { ChatStreamFrame } from '../core/core-contract';

/** One navigation target offered under a help answer (v4 `NavigationLink`). */
export interface NavigationLink {
  url: string;
  label: string;
}

/** The folded state the dialog renders (v4 `StreamingState`, plus two fields). */
export interface HelpStreamState {
  isStreaming: boolean;
  isExecutingTools: boolean;
  streamingContent: string;
  streamingParticipantId: string | null;
  streamingNavigationLinks: NavigationLink[];
  /** Links extracted from help_search results — suggested pages by relevance. */
  suggestedLinks: NavigationLink[];
  error: string | null;
  /**
   * v4's two `return`s out of the read loop (on `error` and on `chainComplete`)
   * made explicit, so the fold stays pure and the consumer can stop reading.
   */
  halted: boolean;
  /**
   * The `messageId`s carried by `done` frames, in arrival order — v4 fires
   * `onMessageComplete(messageId)` here as a side effect; a pure fold records
   * them instead and lets the caller notice the growth.
   */
  completedMessageIds: string[];
}

export function initialHelpStreamState(): HelpStreamState {
  return {
    isStreaming: true,
    isExecutingTools: false,
    streamingContent: '',
    streamingParticipantId: null,
    streamingNavigationLinks: [],
    suggestedLinks: [],
    error: null,
    halted: false,
    completedMessageIds: [],
  };
}

/**
 * Generate a human-readable label from a Quilltap internal URL.
 * e.g., "/settings?tab=chat&section=dangerous-content" → "Settings → Chat → Dangerous Content"
 *
 * Transcribed verbatim from v4 (`useHelpChatStreaming.ts:36-62`) and pinned by a
 * 35-vector capture of v4's real function. Its edges are load-bearing and none
 * of them is obvious: `URLSearchParams` decodes `+` to a space, a FALSY (empty)
 * `tab` is skipped entirely, and a `section` with a leading hyphen splits to an
 * empty first word — so `?section=-lead-ing` renders a DOUBLE space after the
 * arrow. Do not "tidy" any of it.
 */
export function labelFromUrl(url: string): string {
  const [path, query] = url.split('?');

  const pathNames: Record<string, string> = {
    '/settings': 'Settings',
    '/aurora': 'Characters',
    '/salon': 'Chats',
    '/prospero': 'Projects',
    '/profile': 'Profile',
    '/files': 'Files',
    '/setup': 'Setup',
  };

  const basePath = Object.keys(pathNames).find((p) => path.startsWith(p));
  let label = basePath ? pathNames[basePath] : path;

  if (query) {
    const params = new URLSearchParams(query);
    const tab = params.get('tab');
    const section = params.get('section');
    if (tab) {
      label += ` → ${tab.charAt(0).toUpperCase() + tab.slice(1).replace(/-/g, ' ')}`;
    }
    if (section) {
      label += ` → ${section
        .split('-')
        .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
        .join(' ')}`;
    }
  }

  return label;
}

/**
 * Fold one frame into the stream state.
 *
 * The branch ORDER and the fact that they are independent `if`s (never
 * `else if`) are v4's — a single frame legitimately carries several markers,
 * and the later branches overwrite the earlier ones' buffer clears.
 */
export function reduceHelpFrame(prev: HelpStreamState, frame: ChatStreamFrame): HelpStreamState {
  if (prev.halted) return prev;
  let s = prev;

  // Content chunk
  if (frame.content) {
    s = {
      ...s,
      isExecutingTools: false,
      streamingContent: s.streamingContent + frame.content,
    };
  }

  // Turn start (multi-character) — reset the buffer for the new character
  if (frame.turnStart) {
    s = {
      ...s,
      streamingContent: '',
      streamingParticipantId: frame.participantId ?? null,
    };
  }

  // Done event — reset for a potential next character, record the message id
  if (frame.done) {
    s = { ...s, streamingContent: '' };
    if (frame.messageId) {
      s = { ...s, completedMessageIds: [...s.completedMessageIds, frame.messageId] };
    }
  }

  // Navigate event (from help_navigate tool) — collect as a link
  if (frame.toolResult && frame.toolResult.name === 'help_navigate' && frame.toolResult.success) {
    const result = parseToolResult(frame.toolResult.result);
    const navUrl = result?.['url'] || result?.['navigationUrl'];
    if (typeof navUrl === 'string' && navUrl) {
      const link: NavigationLink = { url: navUrl, label: labelFromUrl(navUrl) };
      // Avoid duplicates
      if (!s.streamingNavigationLinks.some((l) => l.url === link.url)) {
        s = { ...s, streamingNavigationLinks: [...s.streamingNavigationLinks, link] };
      }
    }
  }

  // Search results (from help_search tool) — extract URLs as suggested links
  if (frame.toolResult && frame.toolResult.name === 'help_search' && frame.toolResult.success) {
    const result = parseToolResult(frame.toolResult.result);
    // v4: `result?.results || result` — the payload is either `{results: [...]}`
    // or the bare array, and a falsy `results` falls through to the whole body.
    const results = (result?.['results'] as unknown) || (result as unknown);
    if (Array.isArray(results)) {
      const collected = [...s.suggestedLinks];
      for (const item of results) {
        const url = (item as Record<string, unknown> | null)?.['url'];
        if (url && typeof url === 'string' && url.startsWith('/')) {
          // Skip if already in nav links or suggestions
          if (
            !s.streamingNavigationLinks.some((l) => l.url === url) &&
            !collected.some((l) => l.url === url)
          ) {
            const title = (item as Record<string, unknown>)['title'];
            collected.push({
              url,
              label: typeof title === 'string' && title ? title : labelFromUrl(url),
            });
          }
        }
      }
      if (collected.length > 0) {
        s = { ...s, suggestedLinks: collected };
      }
    }
  }

  // Status events (tool execution) — clear intermediate content so the user
  // sees a "working" indicator instead of stale text
  if (frame.status) {
    s = {
      ...s,
      isExecutingTools: true,
      streamingContent: '',
      // v4 `event.status.participantId || prev.streamingParticipantId` — JS
      // `||`, so an empty id falls back too.
      streamingParticipantId: frame.participantId || s.streamingParticipantId,
    };
  }

  // Error — v4 records `event.error` bare and returns out of the loop
  if (frame.error) {
    return { ...s, error: frame.error, isStreaming: false, halted: true };
  }

  // Chain complete — terminal
  if (frame.chainComplete) {
    return { ...s, isStreaming: false, streamingContent: '', halted: true };
  }

  return s;
}

/**
 * v4's `typeof result === 'string' ? JSON.parse(result) : result`, wrapped in
 * the same swallowing `catch`: an unparseable tool result takes the whole
 * branch out silently rather than breaking the stream.
 */
function parseToolResult(raw: unknown): Record<string, unknown> | null {
  try {
    const value = typeof raw === 'string' ? (JSON.parse(raw) as unknown) : raw;
    if (value === null || value === undefined) return null;
    return value as Record<string, unknown>;
  } catch {
    return null;
  }
}
