import { diagramStyleOrDefault, type DiagramStyle } from './diagram-fences';

/**
 * Whose hand a message's diagrams are drawn in. A character's line — an
 * ASSISTANT turn, or a USER line spoken through a user-controlled character —
 * takes that character's `diagramStyle` (absent reads as hand-drawn). The
 * operator speaking as no one, and every Staff sender, draw `clean`.
 *
 * v5 port of v4 `app/salon/[id]/components/message-row/helpers.ts:20-28`
 * (236c04e35). The seat's `character.diagramStyle` arrives on the chat wire
 * through contract C6 (P4.D349); until that lands it is absent, and
 * {@link diagramStyleOrDefault} reads absent as v4's default.
 */
export function resolveMessageDiagramStyle(
  message: { participantId?: string | null; systemSender?: string | null },
  participants: ReadonlyArray<{ id: string; character: { diagramStyle?: unknown } | null }>,
): DiagramStyle {
  if (message.systemSender) return 'clean';
  if (!message.participantId) return 'clean';
  const character = participants.find((p) => p.id === message.participantId)?.character;
  return character ? diagramStyleOrDefault(character.diagramStyle) : 'clean';
}
