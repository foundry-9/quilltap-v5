import { ChangeDetectionStrategy, Component, output } from '@angular/core';

import { Icon } from '../../ui/icon';

/**
 * The sidebar's "Edit Content" section (v4 `ChatSidebar.tsx:1600-1672`
 * `EditContentSection`) — the drawer that rewrites what is already in the
 * transcript.
 *
 * The accordion has declared an `'edit'` id since P4.9H1 (`chat-sidebar.ts`),
 * but nothing rendered it: this is that section arriving, in v4's order.
 *
 * Ported: **Replace** (v4 :1624, `SearchReplaceModal`) and **Bulk Replace**
 * (:1636, `BulkCharacterReplaceModal`), in v4's order.
 *
 * ## Tier-3 deferrals (LOUD — rendered nowhere, nothing stubbed)
 *
 * v4's other two entries are memory maintenance, not text editing, and neither
 * has a v5 verb behind it:
 *
 * - **Re-extract Memories** (v4 :1648) — `onReextractMemoriesClick`, which v4
 *   wires to its re-extraction job. v5's memory-extraction handlers run on the
 *   turn path only; there is no operator-initiated re-extraction verb.
 * - **Delete Memories (n)** (v4 :1660) — `onDeleteChatMemoriesClick`, and its
 *   label carries a per-chat memory count v5's chat read does not project.
 *
 * Both belong to the Commonplace Book family rather than to `p4.9e3`; they are
 * recorded here so the section's shape is legible against v4's, and in
 * `m6-screen-parity.md`.
 *
 * ### The shape whoever lands Delete Memories inherits (P4.D177, bug 128)
 *
 * v4's `useMemoryActions.handleDeleteChatMemories`
 * (`app/salon/[id]/hooks/useMemoryActions.ts`) is worth transcribing
 * verbatim, not reinventing, once a v5 memory-count key exists:
 *
 * 1. **Disabled at zero** — the button is `disabled` while the rendered
 *    count is 0 (`.qt-tool-palette-button:disabled` is already themed in v5).
 * 2. **Re-read from the server before confirming** — the click first
 *    `fetch`es the live count (`GET /api/v1/memories?chatId=…`,
 *    `cache:'no-store'`) rather than trusting the subscribed value, because a
 *    dropped realtime connection can leave a stale zero on screen that used
 *    to swallow the click silently (bug 128's OTHER half — the topic this
 *    unit lands un-stales the count in the FIRST place, but a socket that
 *    was down between the extraction and the click still needs this
 *    belt-and-braces re-read); a failed probe falls through to the rendered
 *    count rather than refusing the click outright.
 * 3. **Toast on both outcomes** — a fresh zero after the re-read toasts
 *    "This chat has no memories to delete" instead of opening the confirm
 *    dialog; success toasts "Deleted N memories" (the server's own count,
 *    not the pre-delete one); failure toasts the server's error message.
 * 4. **The confirm dialog quotes the RE-READ count**, never the possibly
 *    stale rendered one.
 */
@Component({
  selector: 'qt-edit-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-chat-sidebar-section qt-chat-sidebar-section-edit flex flex-col gap-2">
      <button
        type="button"
        class="qt-tool-palette-button"
        title="Search and replace in chat"
        (click)="searchReplace.emit()"
      >
        <qt-icon name="search" class="w-4 h-4" />
        <span>Replace</span>
      </button>

      <button
        type="button"
        class="qt-tool-palette-button"
        title="Bulk re-attribute messages between characters"
        (click)="bulkReplace.emit()"
      >
        <qt-icon name="swap" class="w-4 h-4" />
        <span>Bulk Replace</span>
      </button>
    </div>
  `,
})
export class EditSection {
  readonly searchReplace = output<void>();
  readonly bulkReplace = output<void>();
}
