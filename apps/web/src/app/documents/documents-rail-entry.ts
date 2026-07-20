/**
 * `qt-documents-rail-entry` — the left-rail "Document Mode" opener (port of v4
 * `sidebar-footer.tsx:255-262` + `handleSelectDocument:163-197`, baseline
 * `7e6d13e5`).
 *
 * A single rail button opens the {@link DocumentPicker} in its standalone
 * (chat-less) variant. A selection builds a {@link DocumentStandaloneTabPayload}
 * — keying its `docKey` via {@link standaloneDocKey} so reopening the same file
 * focuses the existing tab — and opens a `document-standalone` workspace tab. In
 * routed (flag-off, or not-on-/workspace) mode it funnels into the workspace via
 * a `?open=document-standalone&…` intent, exactly as v4's legacy-shell fallback.
 *
 * Mounted in `shell.ts` by the UNIFIER (§W.2), alongside the Wardrobe entry.
 *
 * @module documents/documents-rail-entry
 */

import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';

import { Icon } from '../ui/icon';
import { isWorkspaceTabsEnabled } from '../workspace/workspace-flag';
import {
  standaloneDocKey,
  type DocumentStandaloneTabPayload,
} from '../workspace/workspace-contract';
import { WorkspaceService } from '../workspace/workspace.service';
import { DocumentPicker, type DocumentSelection } from './document-picker';

@Component({
  selector: 'qt-documents-rail-entry',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, DocumentPicker],
  template: `
    <button
      type="button"
      class="qt-collapsed-nav-button"
      title="Document Mode"
      aria-label="Document Mode"
      (click)="openPicker()"
    >
      <qt-icon name="file-plus" class="w-7 h-7" />
    </button>

    @if (showPicker()) {
      <qt-document-picker
        [chatId]="null"
        (selectDocument)="handleSelect($event)"
        (close)="showPicker.set(false)"
      />
    }
  `,
})
export class DocumentsRailEntry {
  private readonly workspace = inject(WorkspaceService);
  private readonly router = inject(Router);

  protected readonly showPicker = signal(false);

  protected openPicker(): void {
    this.showPicker.set(true);
  }

  /**
   * Open the picked document as a standalone (chat-less) Document Mode tab. No
   * conversation is attached, so nothing is announced when it's edited.
   */
  protected handleSelect(sel: DocumentSelection): void {
    this.showPicker.set(false);
    // The standalone surface has no project context; `project` can't arrive from
    // a chat-less picker, but map it defensively (v4 `handleSelectDocument`).
    const scope: DocumentStandaloneTabPayload['scope'] =
      sel.scope === 'document_store' ? 'document_store' : 'general';

    if (isWorkspaceTabsEnabled() && this.onWorkspace()) {
      const payload: DocumentStandaloneTabPayload = {
        docKey: standaloneDocKey(scope, sel.mountPoint ?? null, sel.filePath),
        scope,
        mountPoint: sel.mountPoint ?? null,
        filePath: sel.filePath,
        targetFolder: sel.targetFolder,
        displayTitle: sel.title,
      };
      const title = sel.filePath?.split('/').pop() || sel.title || 'New Document';
      this.workspace.openTab('document-standalone', payload, { title });
      return;
    }

    // Legacy shell: funnel into the workspace with an `?open=` intent, which
    // mints the tab (and its docKey) on arrival (v4 :192-196).
    const sp = new URLSearchParams({ open: 'document-standalone', scope });
    if (sel.mountPoint) sp.set('mountPoint', sel.mountPoint);
    if (sel.filePath) sp.set('filePath', sel.filePath);
    if (sel.targetFolder) sp.set('targetFolder', sel.targetFolder);
    void this.router.navigateByUrl(`/workspace?${sp.toString()}`);
  }

  /** True when the workspace host is the live surface (its openTab is honoured). */
  private onWorkspace(): boolean {
    return this.router.url.split('?')[0] === '/workspace';
  }
}
