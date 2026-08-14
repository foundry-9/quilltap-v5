import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { triggerUrlDownload } from '../../core/download-utils';
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
 *
 * **Merge In… is LIVE** (v4 :1566, P4.9E3C) — and, like v4, it is hidden in an
 * autonomous room.
 *
 * **Export is LIVE** (v4 :1511-1515,1578-1586, P4.9E3C; the download-helper
 * fix absorbed from v4 4.8.2 by P4.D72): the entry anchor-clicks the byte
 * route, exactly as v4 does — there is no dialog and no fetch, the browser's
 * own download machinery takes it from there. It navigated the window until
 * 4.8.2, which downloads in a browser but walks the app window onto the API
 * endpoint in a native shell. The route itself is P4.9E3B's
 * (`GET /chats/{id}?action=export`).
 *
 * **Export Markdown is LIVE** (v4 :1515-1519,1595-1604, P4.d28 over v4
 * `b3ee00f1`): the readable transcript, sitting between Export and Gallery in
 * v4's own order. v4 reaches it through `triggerUrlDownload` rather than a
 * navigation, so the port does too.
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

      @if (!isAutonomousRoom()) {
        <button
          type="button"
          class="qt-tool-palette-button"
          title="Merge another conversation's characters and summary into this one"
          (click)="mergeIn.emit()"
        >
          <qt-icon name="user-plus" class="w-4 h-4" />
          <span>Merge In…</span>
        </button>
      }

      <button
        type="button"
        class="qt-tool-palette-button"
        title="Export chat"
        (click)="onExport()"
      >
        <qt-icon name="download" class="w-4 h-4" />
        <span>Export</span>
      </button>

      <button
        type="button"
        class="qt-tool-palette-button"
        title="Export the conversation as a readable Markdown transcript"
        (click)="onExportMarkdown()"
      >
        <qt-icon name="file" class="w-4 h-4" />
        <span>Export Markdown</span>
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
  readonly mergeIn = output<void>();
  readonly openState = output<void>();
  readonly openGallery = output<void>();

  /**
   * v4 `handleExport` (`ChatSidebar.tsx:1511-1515`) — an anchor-click through
   * `triggerUrlDownload`, the same helper the Markdown entry beside it uses.
   *
   * It was a bare `window.location.href = …` in both apps until v4 4.8.2: in a
   * browser that starts a download, but in a native shell it NAVIGATES the app
   * window to the API endpoint instead. v5 has the same exposure in the Tauri
   * webview on the `qtap://` origin, so the fix ports. The server still names
   * the file after the chat title via `Content-Disposition`; the filename here
   * is only the anchor's fallback.
   */
  protected onExport(): void {
    triggerUrlDownload(
      `/api/v1/chats/${encodeURIComponent(this.chatId())}?action=export`,
      'chat_export.qtap',
    );
  }

  /**
   * v4 `handleExportMarkdown` (`ChatSidebar.tsx:1517-1521`) — the same
   * anchor-click. The filename passed here is only that anchor's fallback: the
   * server names the real file after the chat via `Content-Disposition`.
   */
  protected onExportMarkdown(): void {
    triggerUrlDownload(
      `/api/v1/chats/${encodeURIComponent(this.chatId())}?action=export-markdown`,
      'chat_transcript.md',
    );
  }
}
