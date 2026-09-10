import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { injectChatGallery } from '../chat-gallery.api';
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
 * - **Continue Elsewhere** (v4 :1554) — the continue-chat flow is unported,
 *   and would seed `conciergeState` from the source chat (v4 `303288fb4`:
 *   `SalonView.tsx:1730` passes `initialConciergeState={getConciergeState(chat)}`
 *   through `NewChatModal` → `new-chat-provider` → `useNewChat`, so a spicy
 *   conversation that changes venue stays spicy by default). P4.D149 landed the
 *   New Chat picker itself but has NO counterpart for this seeding — there is no
 *   continuation entrance to seed from. Whichever lane ports Continue Elsewhere
 *   carries it.
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
 * ## Gallery — the retired divergence (P4.D176, `78b381a96` bug 129)
 *
 * This entry used to carry a recorded divergence: *"v4 gates Gallery on
 * `chatPhotoCount > 0` … v5 has no per-chat photo count on the chat read, so
 * the entry is unconditional and unnumbered."* v4's `86d59660c` retires the
 * gate ITSELF (bug 129 — the old count read `?action=files`, an action
 * `handleGet` never dispatched, so the gate never opened): v4's post-fix
 * shape is what v5 already had, PLUS a count — ungated, always rendered,
 * labelled from the SAME `chatGallery` query the grid reads
 * (`injectChatGallery`, `chat/chat-gallery.api.ts`), so the count and the grid
 * are one answer and cannot disagree. The invariant bug 129 teaches carries
 * forward even though v5 never had its shape: a control gated (or numbered)
 * off a fetch needs that fetch to reach an endpoint that EXISTS — before
 * `chatGallery` lands server-side (P4.D174), the query errors and the label
 * stays unnumbered rather than claiming a count it does not have.
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
        title="Every image in this conversation"
        (click)="openGallery.emit()"
      >
        <qt-icon name="image" class="w-4 h-4" />
        <span>{{ galleryLabel() }}</span>
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
   * v4 `ChatSidebar.tsx:1676` `Gallery ({galleryCount})` — read from the SAME
   * `chatGallery` query the grid draws (`injectChatGallery`), NEVER from any
   * chat-read field. `hasData()` is false until the verb answers (pre-P4.D174,
   * or still loading), which is when the label stays unnumbered.
   */
  private readonly gallery = injectChatGallery(() => this.chatId());
  protected readonly galleryLabel = computed(() =>
    this.gallery.hasData() ? `Gallery (${this.gallery.total()})` : 'Gallery',
  );

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
