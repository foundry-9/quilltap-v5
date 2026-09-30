import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon } from '../../ui/icon';

/**
 * The sidebar's "Edit Content" section (v4 `ChatSidebar.tsx:1737-1799`
 * `EditContentSection`) — the drawer that rewrites what is already in the
 * transcript, in v4's order: **Replace** (`SearchReplaceModal`), **Bulk
 * Replace** (`BulkCharacterReplaceModal`), **Re-extract Memories** and
 * **Delete Memories (n)**.
 *
 * The memory pair is v4's `useMemoryActions` (`app/salon/[id]/hooks/`), hosted
 * by the conversation over `screens/salon/memory-sidebar.api.ts`. The section
 * only renders the buttons: Delete is `disabled` while the rendered count is 0
 * (v4 `:1781-1795`, title `This chat has laid down no memories yet` /
 * `Delete chat memories`), and the handler re-reads the live count before it
 * confirms, so a stale rendered zero never swallows a click — `memoryCount` is
 * a hint the memories realtime topic keeps fresh, not the authority.
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

      <button
        type="button"
        class="qt-tool-palette-button"
        title="Re-extract memories"
        (click)="reextractMemories.emit()"
      >
        <qt-icon name="refresh" class="w-4 h-4" />
        <span>Re-extract Memories</span>
      </button>

      <button
        type="button"
        class="qt-tool-palette-button qt-tool-palette-button-danger"
        [disabled]="memoryCount() === 0"
        [title]="memoryCount() === 0 ? 'This chat has laid down no memories yet' : 'Delete chat memories'"
        (click)="deleteMemories.emit()"
      >
        <qt-icon name="trash" class="w-4 h-4" />
        <span>Delete Memories ({{ memoryCount() }})</span>
      </button>
    </div>
  `,
})
export class EditSection {
  readonly searchReplace = output<void>();
  readonly bulkReplace = output<void>();
  /** The rendered per-chat memory count (v4 `chatMemoryCount`). */
  readonly memoryCount = input(0);
  readonly reextractMemories = output<void>();
  readonly deleteMemories = output<void>();
}
