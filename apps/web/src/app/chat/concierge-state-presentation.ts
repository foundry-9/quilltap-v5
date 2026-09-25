import type { IconName } from '../ui/icon';
import type { ConciergeProvenance, ConciergeReason, ConciergeState } from './concierge-state';

/**
 * How the Concierge's three states are *shown* — the single source for every
 * word, icon and tone a UI puts on screen (v4
 * `lib/services/dangerous-content/concierge-state-presentation.ts`, new at
 * `c43d3b1b4`, three states since `4d370a90f` #75, the `info` tone retired at
 * `3b463d6b1` #76).
 *
 * Its sibling, `concierge-state.ts`, is the single source for *deriving* a
 * state (and its provenance) from a chat. This module never derives anything;
 * hand it a {@link ConciergeState} — and, for Unmoderated, who put the chat
 * there — and it hands back the presentation.
 *
 * Provenance is a note, never a colour: Unmoderated is one tone whoever set
 * it, and only the helper sentence changes.
 *
 * It exists because the same states were being described in three places
 * with three different sets of words — the Salon header pill's `title`
 * strings, the sidebar's helper sentences, and the list asterisk's terse
 * "Flagged as dangerous" — and a fourth consumer by copy-paste is how the copy
 * drifts. The `detail` sentences below are the sidebar's, moved verbatim: they
 * are the fullest statement of each state and already in voice.
 *
 * Shared contract §B: the table lives ONCE, here. v4's module has no server
 * consumer (its readers are the sidebar, the Salon header and the mark), so
 * the Rust core builds no twin and emits no presentation string. What the two
 * sides DO share is the predicate name — `conciergeStateUsesUncensoredRoute`
 * in `concierge-state.ts`, `concierge_state_uses_uncensored_route` in the
 * core's `chat_override`.
 *
 * Every string here is pinned against v4's REAL module, executed and emitted
 * to `concierge-state-presentation.v4.json` by
 * `harness/oracle/cases/concierge-presentation.mjs` — see the spec.
 */

/**
 * The colour families the states speak in. `danger` is the red of the
 * uncensored desk, `muted` the grey of a chat locked to the ordinary desks,
 * `success` the green of a watch being kept. (`info`, the blue of the retired
 * operator-asserted Uncensored state, was removed in v4's phase 4 with its
 * CSS — and here with its two `_chat.css` rules.)
 */
export type ConciergeTone = 'danger' | 'muted' | 'success';

export interface ConciergeStatePresentation {
  /** Short label — badge text, aria-label, tooltip title. */
  label: string;
  /** Canonical icon for the state (the sidebar's icon, the badge's glyph). */
  icon: IconName;
  /** Colour family; see {@link conciergeToneSuffix} and {@link conciergeToneTextClass}. */
  tone: ConciergeTone;
  /** The full "what this means" sentence, in Quilltap's voice. */
  detail: string;
  /** Where to change it; appended to tooltips outside the sidebar. */
  hint: string;
}

/** Where every state is changed from — one sentence, said once. */
const CHANGE_HINT = "Change it from the Salon sidebar's Chat section.";

/**
 * THE table. Three states, three presentations; every badge, mark, icon and
 * helper sentence in the application reads from here, so a copy edit lands
 * everywhere at once. Unmoderated's `detail` is the operator's variant;
 * {@link describeConciergeState} swaps in the Concierge's when he set it.
 */
export const CONCIERGE_STATE_PRESENTATION: Record<ConciergeState, ConciergeStatePresentation> = {
  moderated: {
    label: 'Moderated',
    icon: 'eye',
    tone: 'success',
    detail:
      'The Concierge sends everything to the usual providers first, and to the uncensored desk only when one of them refuses. After enough refusals he moves the whole chat himself.',
    hint: CHANGE_HINT,
  },
  unmoderated: {
    label: 'Unmoderated',
    icon: 'eye-off',
    tone: 'danger',
    detail:
      'You have opened the uncensored door yourself. Nothing here goes near a moderated provider.',
    hint: CHANGE_HINT,
  },
  locked: {
    label: 'Locked',
    icon: 'shield',
    tone: 'muted',
    detail:
      'Only the usual providers, ever. If one refuses, the refusal stands. For the chat that must never reach an uncensored model.',
    hint: CHANGE_HINT,
  },
};

const NUMBER_WORDS = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten'];

/**
 * The Concierge's own variant of Unmoderated's helper sentence, by why he
 * moved the chat. A migrated chat the classifier had flagged reads as the
 * classifier's.
 */
function conciergeMovedDetail(
  reason: ConciergeReason | null | undefined,
  refusalCount?: number | null,
): string {
  if (reason === 'refusals') {
    const n = refusalCount ?? 0;
    const counted =
      n > 0
        ? ` after ${n < NUMBER_WORDS.length ? NUMBER_WORDS[n] : n} ${n === 1 ? 'refusal' : 'refusals'}`
        : ' after the usual providers refused it';
    return `The Concierge moved this chat to the uncensored desk${counted}. Set it back to Moderated if you disagree.`;
  }
  return 'The Concierge moved this chat to the uncensored desk on reading the conversation. Set it back to Moderated if you disagree.';
}

/**
 * Tone → the class suffix shared by the `qt-danger-badge` and
 * `qt-concierge-mark` families. `danger` is the base rule, so it suffixes with
 * nothing; `success` has no modifier in either family (Moderated draws no badge
 * and no mark) and likewise falls through to the base.
 */
export function conciergeToneSuffix(tone: ConciergeTone): '' | '-muted' {
  if (tone === 'muted') return '-muted';
  return '';
}

/**
 * Tone → the text-colour utility class, for the icons that carry a colour of
 * their own (the sidebar's state glyph). Spelled out one branch at a time
 * rather than interpolated, so `check-qt-classes` can see each class name.
 * (v4's own doc comment names the family in prose for the same reason: the
 * scanner stops at a `*` and reads a bare, ruleless prefix.)
 */
export function conciergeToneTextClass(tone: ConciergeTone): string {
  switch (tone) {
    case 'muted':
      return 'qt-text-muted';
    case 'success':
      return 'qt-text-success';
    default:
      return 'qt-text-danger';
  }
}

/** Everything a tooltip needs, in the order it is read. */
export interface ConciergeStateDescription {
  /** The state's short label — the tooltip's title line. */
  title: string;
  /** The full sentence, in the variant the provenance calls for. */
  detail: string;
  /** The classifier's categories — Unmoderated by the classifier only, and only when it has any. */
  categories: string[] | null;
  /** Where to change the state. */
  hint: string;
}

/**
 * The provenance note a tooltip or helper text needs (v4
 * `ConciergeProvenanceNote`). A NAMED object type on purpose: v4 moved the
 * categories from the 2nd argument to the 3rd, and a stale two-argument call
 * passing a `string[]` second must not compile (an array has no `setBy`, but
 * a weak all-optional type would still accept it — see the guard below).
 */
export interface ConciergeProvenanceNote {
  setBy?: ConciergeProvenance;
  reason?: ConciergeReason | null;
  /** Refusals on the ledger — for "after N refusals". Only the header pill and the sidebar pass it. */
  refusalCount?: number | null;
  /** Never set: makes an array (a stale categories argument) unassignable. */
  length?: never;
}

/**
 * Describe a state for a tooltip, helper text or an accessible summary.
 *
 * Unmoderated picks its sentence by provenance: the operator's own, or the
 * Concierge's with his reason. `dangerCategories` is surfaced only when the
 * classifier's verdict is what moved the chat — they are its reasons; on any
 * other state they are a preserved artefact of an earlier scan.
 */
export function describeConciergeState(
  state: ConciergeState,
  provenance: ConciergeProvenanceNote = {},
  dangerCategories?: string[],
): ConciergeStateDescription {
  const presentation = CONCIERGE_STATE_PRESENTATION[state];
  const byConcierge = state === 'unmoderated' && provenance.setBy === 'concierge';
  const detail = byConcierge
    ? conciergeMovedDetail(provenance.reason, provenance.refusalCount)
    : presentation.detail;
  const categories =
    byConcierge &&
    provenance.reason !== 'refusals' &&
    dangerCategories &&
    dangerCategories.length > 0
      ? dangerCategories
      : null;

  return {
    title: presentation.label,
    detail,
    categories,
    hint: presentation.hint,
  };
}
