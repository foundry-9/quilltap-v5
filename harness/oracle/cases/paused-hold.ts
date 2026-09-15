/**
 * Oracle case: the paused-chat hold rule (P4.D186; v4 `31436bae4` bug 137,
 * `lib/services/chat-message/paused-hold.ts` `shouldHoldUserTurnForPause`).
 *
 * Drives v4's REAL export over the FULL input grid — `isContinueMode` ×
 * `chatIsPaused` × `neverPauseForUser` in all three of its states (absent /
 * `false` / `true`, because v4's guard is `=== true` and an optional field's
 * absent arm is not the same input as an explicit `false`). Twelve rows, exact
 * booleans, computed on both sides and never transcribed.
 *
 * v4's own four unit shapes are a SUBSET of the grid and are labelled with the
 * `v4_` prefix the Rust side asserts by name.
 *
 * Run from inside the server checkout (a pinned worktree under PIN REQUIRED):
 *   cd ~/source/quilltap-server   # (pinned: pass --v4 <pin> to the sweep driver)
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/paused-hold.ts \
 *     > /tmp/oracle-paused-hold.ndjson
 */

import { shouldHoldUserTurnForPause } from '@/lib/services/chat-message/paused-hold';

/** The three states of v4's optional `neverPauseForUser`. */
const NEVER_PAUSE: Array<{ tag: string; value: boolean | undefined }> = [
  { tag: 'absent', value: undefined },
  { tag: 'false', value: false },
  { tag: 'true', value: true },
];

/**
 * v4's four unit-test shapes, keyed by the grid coordinates they occupy, so the
 * corpus floor is derived from the grid rather than duplicated beside it.
 */
const V4_SHAPES: Record<string, string> = {
  'continue=false|paused=true|never=absent': 'v4_holds_a_typed_message_while_paused',
  'continue=false|paused=false|never=absent': 'v4_lets_a_typed_message_through_when_not_paused',
  'continue=true|paused=true|never=absent': 'v4_never_holds_a_continue_mode_summons',
  'continue=false|paused=true|never=true': 'v4_never_holds_an_autonomous_room_turn',
};

for (const isContinueMode of [false, true]) {
  for (const chatIsPaused of [false, true]) {
    for (const never of NEVER_PAUSE) {
      const coords =
        `continue=${isContinueMode}|paused=${chatIsPaused}|never=${never.tag}`;
      // The absent arm must omit the key entirely — `{ neverPauseForUser:
      // undefined }` and no key at all are the same to `=== true`, but only the
      // omission models an options bag that never carried the field.
      const input =
        never.value === undefined
          ? { isContinueMode, chatIsPaused }
          : { isContinueMode, chatIsPaused, neverPauseForUser: never.value };
      const hold = shouldHoldUserTurnForPause(input);
      process.stdout.write(
        JSON.stringify({
          label: V4_SHAPES[coords] ?? `grid_${coords}`,
          isContinueMode,
          chatIsPaused,
          neverPauseForUser: never.tag,
          hold,
        }) + '\n',
      );
    }
  }
}
