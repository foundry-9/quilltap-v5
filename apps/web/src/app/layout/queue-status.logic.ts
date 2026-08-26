/**
 * The same-tab kick for the toolbar chips (v4
 * `components/layout/queue-status-badges.tsx`, `f3892158d`'s rewrite).
 *
 * Optional since the realtime hints landed — the chips are pushed to by the
 * server and fall back to their own heartbeat, and will notice the work
 * regardless. Call this after an action you know enqueues something, purely so
 * the chip lights within this tab's next frame instead of within a round trip.
 *
 * **Kept, not retired.** v4's `f3892158d` rewrite explicitly holds on to this
 * ("`notifyQueueChange()` remains as an instant same-tab kick after a
 * known-enqueuing action, but nothing depends on it any more") — only its
 * MEANING changed: the listener now invalidates the jobs query key instead of
 * driving a bespoke re-poll. A window event is still the right mechanism for
 * the same reason it was in v4: any code can fire it without a dependency on
 * the badges. What DID go with the rewrite is v5's own invention on top of it —
 * the `NavigationEnd` stop-and-refire, which existed only because the poller
 * was a hand-rolled `setInterval`.
 *
 * @module layout/queue-status.logic
 */

/** v4 `QUEUE_CHANGE_EVENT` — the same DOM CustomEvent name. */
export const QUEUE_CHANGE_EVENT = 'quilltap:queue-change';

/** v4 `notifyQueueChange()`. */
export function notifyQueueChange(): void {
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(QUEUE_CHANGE_EVENT));
  }
}
