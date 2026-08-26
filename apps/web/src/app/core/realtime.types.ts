/**
 * The realtime invalidation hint — v5's twin of v4 `lib/schemas/realtime.types.ts`.
 *
 * Deliberately tiny: a hint says *which slice of server state changed*, never
 * *what it changed to*. The dispatch API stays the single source of truth for
 * the data itself; the hint only says when to look again.
 *
 * ## The mechanism divergence, recorded (P4.D125 §Shared contract §B.1)
 *
 * v4 carries these frames on a NEW multiplexed WebSocket at
 * `/api/v1/system/realtime/stream`. v5 does not open one. The hints ride the
 * EXISTING event channel — the engine broadcast → SSE `GET /api/events` in HTTP
 * mode, → `quilltap://event` in the Tauri shell — because the locked
 * transport-agnostic boundary says streaming only ever happens on the `Event`
 * channel, and v4's own decision 2 says one connection per tab. Opening a
 * second socket would violate both at once.
 *
 * What that costs, and what it doesn't:
 *  - The ping/pong keepalive and the hand-rolled 1 s → 30 s jittered backoff
 *    are WS-protocol legs with no work to do here: `EventSource` reconnects on
 *    its own, and the Tauri pump is in-process. NO-PORT, per leg.
 *  - Everything observable carries: a `connected` status the fallback gating
 *    reads, unknown-topic tolerance, and the catch-up sweep on every
 *    (re)connect / resync.
 *
 * @module core/realtime.types
 */

/**
 * Canonical topic names — verbatim from v4's `REALTIME_TOPICS`, and closed for
 * this round (§B.3).
 *
 * The wire accepts *any* string, so a server that learns a new topic cannot
 * break an older tab; see {@link queryKeysForTopic}, which ignores what it does
 * not recognise.
 */
export const REALTIME_TOPICS = [
  /** Background-job lifecycle and inline activity spans — the toolbar chips. */
  'jobs',
  /** Autonomous-room run state and budgets. */
  'autonomousRooms',
  /** Chats: list membership, detail, per-chat background/state. */
  'chats',
  /** Projects, including their story backgrounds. */
  'projects',
  /** Characters and their prompts/photos. */
  'characters',
  /** Document stores and their indexing/embedding status. */
  'mountPoints',
] as const;

export type RealtimeTopic = (typeof REALTIME_TOPICS)[number];

/**
 * A server→client invalidation hint.
 *
 * `at` is for debugging and log correlation only. Clients must not order,
 * dedupe, or expire on it — the server's clock is not the client's, and the bus
 * coalesces events anyway (§B.2, v4's own wording).
 */
export interface RealtimeHint {
  v: 1;
  /** A query-key namespace name, e.g. 'jobs', 'chats', 'autonomousRooms'. */
  topic: string;
  /** Entity id, when the change is row-scoped rather than collection-wide. */
  id?: string;
  /** Server ms timestamp — debugging only. */
  at: number;
}

/** The protocol version this build speaks. */
export const REALTIME_PROTOCOL_VERSION = 1;

/**
 * Read one frame off the shared event stream as a hint, or `null` if it is not
 * one.
 *
 * §B.5 — the discrimination rule on a SHARED stream: a frame is a hint iff it
 * carries both `topic` and `v`. Chat-stream and creation-progress frames carry
 * `type` / `kind` / their own scope keys and never both of these, so the
 * untagged serde round trip stays unambiguous.
 *
 * Past the discriminator this is v4's `RealtimeEventSchema.safeParse`: a frame
 * that looks like a hint but fails the shape is DROPPED, exactly as v4 drops it
 * (`client.ts`: `if (!parsed.success) return`) — never thrown from inside a
 * stream handler.
 */
export function realtimeHintFromFrame(frame: unknown): RealtimeHint | null {
  if (!frame || typeof frame !== 'object') return null;
  const raw = frame as Record<string, unknown>;
  if (!('topic' in raw) || !('v' in raw)) return null;
  // The Zod shape: v is the literal 1, topic a string, at a number, id an
  // optional string.
  if (raw['v'] !== REALTIME_PROTOCOL_VERSION) return null;
  if (typeof raw['topic'] !== 'string') return null;
  if (typeof raw['at'] !== 'number') return null;
  const id = raw['id'];
  if (id !== undefined && typeof id !== 'string') return null;
  return id === undefined
    ? { v: 1, topic: raw['topic'], at: raw['at'] }
    : { v: 1, topic: raw['topic'], id, at: raw['at'] };
}
