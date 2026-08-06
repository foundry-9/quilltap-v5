import type { MessageDto, SystemSender } from '../core/core-contract';

/**
 * The whisper-visibility rule (v4 `app/salon/[id]/whisper-visibility.ts`, the
 * `a163862c` + `424a7381` re-port). Ported verbatim — the why-comments are
 * load-bearing.
 */

/**
 * Staff whispers that render for the human operator even when "All Whispers"
 * is off, keyed by sender *and* `systemKind`.
 *
 * The distinction is scene content vs. operator machinery. A private roll, a
 * private Run Tool result, and the errors belonging to each are the table's
 * mechanics: they are whispered so the *characters* can't see them, and the
 * person running the table is the audience they exist for. Everything else a
 * Staff member whispers — the Commonplace Book's recall, Carina's answers to a
 * character, the Librarian, the Host — is addressed to a character as part of
 * the scene, and stays behind the toggle like any other whisper the human isn't
 * party to.
 *
 * Keep this narrow, and keep it keyed on the kind. A blanket `systemSender`
 * exemption once leaked the Commonplace Book's recall whispers into the flow;
 * narrowing it to a *sender* still leaked Prospero's `group-context` whispers,
 * which are him telling one character which group shelves they may read —
 * scene machinery addressed to a character, and the highest-volume whisper in
 * the app. Sender alone has now been the wrong granularity twice.
 */
export const OPERATOR_FACING_WHISPER_KINDS: ReadonlyMap<
  NonNullable<SystemSender>,
  ReadonlySet<string>
> = new Map<NonNullable<SystemSender>, ReadonlySet<string>>([
  // The roll itself.
  ['pascal', new Set(['custom-tool-result'])],
  // A private Run Tool result, and the two failure notices Prospero authors on
  // behalf of other subsystems — an error the operator cannot see is an error
  // they cannot act on.
  ['prospero', new Set(['tool-run', 'custom-tool-error', 'carina-error'])],
]);

/** Whether this Staff whisper is operator machinery rather than scene content. */
function isOperatorFacingStaffWhisper(
  msg: Pick<MessageDto, 'systemSender' | 'systemKind'>,
): boolean {
  if (!msg.systemSender) return false;
  const kinds = OPERATOR_FACING_WHISPER_KINDS.get(msg.systemSender);
  if (!kinds) return false;
  // A legacy row with no stored kind keeps the old sender-level behaviour
  // rather than vanishing from a view the operator is used to.
  if (!msg.systemKind) return true;
  return kinds.has(msg.systemKind);
}

/**
 * Whether a message is an ad-hoc announcement the operator wrote themselves
 * (Insert Announcement composer). `systemKind: 'announcement'` is set by
 * exactly one writer — the announcer service — so it is the marker for "the
 * human at the keyboard authored this", whichever Staff member or invented
 * name it wears.
 *
 * These are never hidden and never dimmed. A whispered announcement has no
 * `participantId` to match the author against, so without this it would
 * vanish the instant the operator posted it: they'd type a private aside,
 * send it, and watch nothing appear.
 */
export function isOperatorAuthoredAnnouncement(msg: Pick<MessageDto, 'systemKind'>): boolean {
  return msg.systemKind === 'announcement';
}

/**
 * The display name shown after "whispered to" for one target id (v4
 * `whisper-visibility.ts` `resolveWhisperTargetLabel`).
 *
 * A private user-initiated run — a Pascal custom-tool or a Run Tool call made
 * with `private: true` — is whispered to the *operator's own userId*, chosen
 * deliberately because it is not a participant id, so every character's context
 * filter excludes it. That id is therefore never among the participant names,
 * which are keyed only by character participants, so it used to fall to
 * "unknown" — the operator's own private roll read as "whispered to unknown"
 * (Bug 30). Resolve the operator's own id to "you"; a real participant id
 * resolves to its name; anything else keeps the "unknown" fallback.
 */
export function resolveWhisperTargetLabel(
  targetId: string,
  participantNames: Record<string, string> | undefined,
  currentUserId: string | null | undefined,
): string {
  if (currentUserId && targetId === currentUserId) return 'you';
  return participantNames?.[targetId] || 'unknown';
}

/** The message fields the rule reads. */
type WhisperMessage = Pick<
  MessageDto,
  'systemSender' | 'systemKind' | 'participantId' | 'targetParticipantIds'
>;

export interface WhisperAudience {
  /** The "All Whispers" toggle: when on, nothing is filtered. */
  showAllWhispers: boolean;
  /** Participant ids the human controls — they see their own whispers either way. */
  userParticipantIds: ReadonlySet<string>;
}

/**
 * Whether a message belongs in the human's rendered flow.
 *
 * This governs display only. What each character can see is decided
 * server-side from `targetParticipantIds` when their context is built — a
 * message shown here was never added to anyone's context by being shown.
 */
export function isMessageVisibleToOperator(
  msg: WhisperMessage,
  { showAllWhispers, userParticipantIds }: WhisperAudience,
): boolean {
  // Not a whisper at all — public scene content.
  if (!msg.targetParticipantIds || msg.targetParticipantIds.length === 0) return true;

  if (showAllWhispers) return true;

  // The operator's own aside, whoever it is signed by.
  if (isOperatorAuthoredAnnouncement(msg)) return true;

  if (isOperatorFacingStaffWhisper(msg)) return true;

  // The human is the whisper's author or one of its targets.
  if (msg.participantId && userParticipantIds.has(msg.participantId)) return true;
  if (msg.targetParticipantIds.some((id) => userParticipantIds.has(id))) return true;

  return false;
}

/**
 * Whether a rendered whisper is being read only because "All Whispers" is
 * on — the operator is neither its author nor one of its targets (v4
 * `VirtualizedMessageList.tsx:358-373`). Dimmed to 0.6 opacity in the
 * stylesheet while keeping the whisper border and label legible, so the
 * privacy status stays visible without the reader mistaking it for a line
 * addressed to them.
 *
 * Staff whispers kept visible by {@link isOperatorFacingStaffWhisper} (a
 * private roll, a private tool run) and the operator's own authored
 * announcements are NEVER overheard — dimming what was deliberately shown
 * would undercut the reason it was shown. Since only a message
 * {@link isMessageVisibleToOperator} lets through ever reaches a row, a
 * message failing every "shown on purpose" test below can only be here
 * because the toggle is on; this function needs no `showAllWhispers`
 * parameter of its own.
 */
export function isOverheardWhisper(
  msg: Pick<MessageDto, 'systemSender' | 'systemKind' | 'participantId' | 'targetParticipantIds'>,
  userParticipantIds: ReadonlySet<string>,
): boolean {
  if (!msg.targetParticipantIds || msg.targetParticipantIds.length === 0) return false;
  if (msg.systemSender) return false;
  if (isOperatorAuthoredAnnouncement(msg)) return false;
  if (msg.participantId && userParticipantIds.has(msg.participantId)) return false;
  if (msg.targetParticipantIds.some((id) => userParticipantIds.has(id))) return false;
  return true;
}
