import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { CopyChatIdButton } from '../../ui/copy-chat-id-button';
import { Icon } from '../../ui/icon';

/**
 * The sidebar's "Organize" section (v4 `ChatSidebar.tsx:1499-1601`
 * `OrganizeSection`): the housekeeping entries for one conversation.
 *
 * Ported: **Edit Enclave** (autonomous rooms only), **Copy ID** (v4's palette
 * livery), **State…** — which closes the state-cascade round's deferred
 * chat-tier State-Editor opener, the modal itself having shipped with P4.6be —
 * and **Gallery**.
 *
 * ## Tier-3 deferrals (LOUD — rendered nowhere, nothing stubbed)
 *
 * These belong to `p4.9e3`. **Their server halves are no longer the blocker:**
 * P4.9E3A (2026-07-26) landed the whole chat-admin verb family, so what is
 * missing here is UI over a live boundary, not a port.
 *
 * - **Continue Elsewhere** (v4 :1554) — the continue-chat flow is unported.
 * - **Merge In…** (v4 :1566) — `MergeConversationModal` is unported; its verb
 *   (`ChatMergeConversation`, over the ported `apply_chat_merge`) IS live.
 * - **Export** (v4 :1578) — `GET /chats/{id}?action=export` has no v5 verb. The
 *   one item here whose server half is genuinely still missing.
 *
 * **Rename is LIVE** (v4 :1530, P4.9E3C 2026-07-27) — and with it
 * `regenerate-title`, which v4 exposes through no button of its own: the route
 * fires as a side effect of ticking "Use automatic naming" inside that dialog
 * (`ChatRenameModal.tsx:52,184-192`), so v5's live verb had no reachable caller
 * until it landed (dogfood walk 2026-07-27).
 *
 * ## One reduction (recorded)
 *
 * v4 gates Gallery on `chatPhotoCount > 0` and shows the count in the label
 * (`Gallery (3)`). v5 has no per-chat photo count on the chat read, so the entry
 * is unconditional and unnumbered — which is what v5's header entry already did
 * before this section reclaimed it.
 */
@Component({
  selector: 'qt-organize-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, CopyChatIdButton],
  template: `
    <div class="qt-chat-sidebar-section qt-chat-sidebar-section-organize flex flex-col gap-2">
      @if (isAutonomousRoom()) {
        <button
          type="button"
          class="qt-tool-palette-button"
          title="Edit this enclave’s schedule, budget, and visibility"
          (click)="editEnclave.emit()"
        >
          <qt-icon name="settings" class="w-4 h-4" />
          <span>Edit Enclave</span>
        </button>
      }

      <qt-copy-chat-id-button [chatId]="chatId()" variant="palette" />

      <button
        type="button"
        class="qt-tool-palette-button"
        title="Rename chat"
        (click)="rename.emit()"
      >
        <qt-icon name="pencil" class="w-4 h-4" />
        <span>Rename</span>
      </button>

      <button
        type="button"
        class="qt-tool-palette-button"
        title="View/edit chat state"
        (click)="openState.emit()"
      >
        <qt-icon name="database" class="w-4 h-4" />
        <span>State…</span>
      </button>

      <button
        type="button"
        class="qt-tool-palette-button"
        title="View gallery"
        (click)="openGallery.emit()"
      >
        <qt-icon name="image" class="w-4 h-4" />
        <span>Gallery</span>
      </button>
    </div>
  `,
})
export class OrganizeSection {
  readonly chatId = input.required<string>();
  /** v4 `chat?.chatType === 'autonomous'` — gates the Edit Enclave entry. */
  readonly isAutonomousRoom = input(false);

  readonly editEnclave = output<void>();
  readonly rename = output<void>();
  readonly openState = output<void>();
  readonly openGallery = output<void>();
}
