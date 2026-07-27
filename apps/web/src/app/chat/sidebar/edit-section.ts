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
 * Ported: **Bulk Replace** (v4 :1636, `BulkCharacterReplaceModal`). **Replace**
 * (:1624, `SearchReplaceModal`) arrives with that dialog — the section leads with
 * the entry it can actually answer rather than shipping a button that does
 * nothing.
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
  readonly bulkReplace = output<void>();
}
