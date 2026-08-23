/**
 * Oracle case: the attachment anchor selector (P4.D106; v4 `a14a1811` bug 95,
 * `lib/services/chat-message/context-builder.service.ts`
 * `selectAttachmentAnchorIndex`).
 *
 * Drives v4's REAL export over a corpus seeded from v4's own six test shapes
 * (`__tests__/unit/lib/services/chat-message/attachment-anchor.test.ts`) and
 * grown with adversarial shapes — both sides COMPUTED, never transcribed:
 * tier-1's role-blindness (an `isUserTurn` flag on a system/assistant row
 * wins), last-wins within each tier, tier-2's exact `role === 'user'` +
 * id-set membership (case-sensitive role, missing id skipped), and the
 * tier-3 floor / -1 arm.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/attachment-anchor.ts \
 *     > /tmp/oracle-attachment-anchor.ndjson
 */

import { selectAttachmentAnchorIndex } from '@/lib/services/chat-message/context-builder.service';

type Msg = { role: string; metadata?: { messageId?: string; isUserTurn?: boolean } };

interface Case {
  label: string;
  messages: Msg[];
  userTurnIds: string[];
}

const USER_ROW = 'user-row-1';

const CASES: Case[] = [
  // ---- v4's six test shapes, verbatim (the corpus floor) -------------------
  {
    label: 'v4_prefers_new_user_message',
    messages: [
      { role: 'system' },
      { role: 'user', metadata: { messageId: USER_ROW } },
      { role: 'assistant' },
      { role: 'user', metadata: { isUserTurn: true } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    label: 'v4_historical_turn_not_staff_whispers',
    messages: [
      { role: 'assistant' },
      { role: 'user', metadata: { messageId: USER_ROW } },
      { role: 'user', metadata: { messageId: 'librarian-whisper' } },
      { role: 'user', metadata: { messageId: 'host-whisper' } },
      { role: 'user', metadata: { messageId: 'profile-change-whisper' } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    label: 'v4_tool_exchange_trails',
    messages: [
      { role: 'user', metadata: { messageId: USER_ROW } },
      { role: 'user', metadata: { messageId: 'profile-change-whisper' } },
      { role: 'assistant' },
      { role: 'user', metadata: { messageId: 'tool-result' } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    label: 'v4_floor_last_user_role',
    messages: [{ role: 'system' }, { role: 'assistant' }, { role: 'user' }],
    userTurnIds: [],
  },
  {
    label: 'v4_no_user_role_at_all',
    messages: [{ role: 'system' }, { role: 'assistant' }],
    userTurnIds: [],
  },
  {
    label: 'v4_user_turn_id_on_assistant_row',
    messages: [
      { role: 'assistant', metadata: { messageId: USER_ROW } },
      { role: 'user', metadata: { messageId: 'whisper' } },
    ],
    userTurnIds: [USER_ROW],
  },
  // ---- adversarial growth (both sides computed) ----------------------------
  { label: 'empty', messages: [], userTurnIds: [] },
  {
    // Tier 1 checks NO role — an isUserTurn flag on a system row wins.
    label: 'is_user_turn_on_system_row',
    messages: [
      { role: 'system', metadata: { isUserTurn: true } },
      { role: 'user', metadata: { messageId: USER_ROW } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    // Tier 1 last-wins: two isUserTurn flags → the later index.
    label: 'two_is_user_turn_last_wins',
    messages: [
      { role: 'user', metadata: { isUserTurn: true } },
      { role: 'assistant' },
      { role: 'user', metadata: { isUserTurn: true } },
      { role: 'user', metadata: { messageId: 'whisper' } },
    ],
    userTurnIds: [],
  },
  {
    // Tier 1 beats a LATER tier-2 match.
    label: 'tier1_beats_later_tier2',
    messages: [
      { role: 'user', metadata: { isUserTurn: true } },
      { role: 'user', metadata: { messageId: USER_ROW } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    // An explicit false flag is falsy — skipped by tier 1.
    label: 'is_user_turn_false_skipped',
    messages: [
      { role: 'user', metadata: { messageId: USER_ROW } },
      { role: 'user', metadata: { isUserTurn: false, messageId: 'whisper' } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    // Tier 2 last-wins among several genuine turns.
    label: 'tier2_last_of_several_turns',
    messages: [
      { role: 'user', metadata: { messageId: 'turn-a' } },
      { role: 'assistant' },
      { role: 'user', metadata: { messageId: 'turn-b' } },
      { role: 'user', metadata: { messageId: 'whisper' } },
    ],
    userTurnIds: ['turn-a', 'turn-b'],
  },
  {
    // Tier 2's role check is exact lowercase 'user' — an uppercase role with a
    // matching id falls through (and tier 3 skips it too).
    label: 'uppercase_role_falls_through',
    messages: [
      { role: 'USER', metadata: { messageId: USER_ROW } },
      { role: 'user', metadata: { messageId: 'whisper' } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    // A tool-role message with a matching id is not a tier-2 anchor.
    label: 'tool_role_id_match_skipped',
    messages: [
      { role: 'user', metadata: { messageId: 'whisper' } },
      { role: 'tool', metadata: { messageId: USER_ROW } },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    // Metadata present but no messageId → tier 2 skips (id falsy), floor catches.
    label: 'metadata_without_id',
    messages: [
      { role: 'user', metadata: {} },
      { role: 'assistant' },
    ],
    userTurnIds: [USER_ROW],
  },
  {
    // Whispers-only context (nothing in the id set) → the old rule as floor.
    label: 'whispers_only_floor',
    messages: [
      { role: 'assistant' },
      { role: 'user', metadata: { messageId: 'whisper-1' } },
      { role: 'user', metadata: { messageId: 'whisper-2' } },
    ],
    userTurnIds: [],
  },
  {
    // Empty-string id is falsy in the tier-2 guard.
    label: 'empty_string_id_falsy',
    messages: [
      { role: 'user', metadata: { messageId: '' } },
      { role: 'assistant' },
    ],
    userTurnIds: [''],
  },
  {
    // The full bug-95 regenerate shape: history turn + three whispers + tool
    // tail, no new user message.
    label: 'regenerate_shape_with_tool_tail',
    messages: [
      { role: 'system' },
      { role: 'user', metadata: { messageId: 'turn-a' } },
      { role: 'assistant' },
      { role: 'user', metadata: { messageId: USER_ROW } },
      { role: 'user', metadata: { messageId: 'librarian-whisper' } },
      { role: 'user', metadata: { messageId: 'host-whisper' } },
      { role: 'assistant' },
      { role: 'user', metadata: { messageId: 'tool-result' } },
    ],
    userTurnIds: ['turn-a', USER_ROW],
  },
];

for (const c of CASES) {
  const index = selectAttachmentAnchorIndex(c.messages, new Set(c.userTurnIds));
  process.stdout.write(
    JSON.stringify({
      label: c.label,
      messages: c.messages,
      userTurnIds: c.userTurnIds,
      index,
    }) + '\n',
  );
}
