import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  EmbeddedViewRef,
  ViewContainerRef,
  computed,
  effect,
  inject,
  input,
  output,
  TemplateRef,
  viewChild,
} from '@angular/core';

import { SplitLayout } from '../../terminal/split-layout';
import type { OpenDocEntry } from '../../documents/document-mode';
import {
  WORKSPACE_HANDLE,
  WORKSPACE_PORTAL_REGISTRY,
  WORKSPACE_TAB_ID,
  portalKey,
} from '../../workspace/workspace-contract';

/** A live portaled pane: its embedded view (kept CD-attached) + the wrapper the
 * view's root nodes were moved into (the element relocated to the child tab). */
interface PortaledPane {
  view: EmbeddedViewRef<unknown>;
  wrapper: HTMLElement;
}

/**
 * `qt-salon-mode-panes` — the v5 port of v4 `app/salon/[id]/components/SalonModePanes.tsx`.
 *
 * Routes the Salon's Document/Terminal panes either to the legacy in-chat
 * {@link SplitLayout} (routed mode — one focused document, exactly as today) or,
 * when hosted inside the tabbed workspace, into sibling CHILD TABS: one
 * `document` tab per open document + one `terminal` tab per chat. Each pane is
 * an EMBEDDED VIEW created via a {@link ViewContainerRef} (so it stays
 * change-detection-attached and its live PTY/editor state is never re-created);
 * its root nodes are moved into a wrapper element which is relocated (DOM
 * `appendChild`) into the node the child tab registers under `portalKey(...)`.
 * This is the Angular-idiomatic equivalent of v4's React `createPortal` — and,
 * unlike moving `@for`-managed DOM, it does not fight the framework's own
 * reconciliation. Moving a VCR-created view's root nodes is exactly how CDK's
 * `DomPortalOutlet` works; CD keeps updating the (moved) nodes in place.
 *
 * Lifecycle THIS lane drives (from the document set): opening a document spawns
 * its child tab; closing the document (it leaves `documentEntries`) closes the
 * tab. The REVERSE — closing the child tab closing the document / toggling the
 * terminal off — is driven from the portal registry (the p4.9j unification
 * wire): a child tab's `TabPortalHost` unregisters its node ONLY when the tab
 * is closed (keep-alive tabs stay mounted hidden), so a previously-seen portal
 * key with no node IS the close-tab signal. v4 read `ws.state.tabs` for this;
 * v5's reduced {@link WorkspaceHandle} exposes no tab map, and the registry
 * disappearance is the equivalent. The parent-Salon-tab cascade itself is
 * reducer-side (lane J1's `CLOSE_TAB` child cascade). Inert in routed mode (the
 * three workspace tokens resolve null ⇒ the SplitLayout branch, byte-identical).
 */
@Component({
  selector: 'qt-salon-mode-panes',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'flex flex-col flex-1 min-h-0 min-w-0' },
  imports: [SplitLayout, NgTemplateOutlet],
  template: `
    @if (!inWorkspace()) {
      <qt-split-layout
        [mode]="mode()"
        [dividerPosition]="dividerPosition()"
        [rightPaneVerticalSplit]="rightPaneVerticalSplit()"
        [chatContent]="chatContent()"
        [documentContent]="focusedDoc() ? focusedDocTpl : null"
        [terminalContent]="terminalContent()"
        (dividerPositionChange)="dividerPositionChange.emit($event)"
        (rightPaneVerticalSplitChange)="rightPaneVerticalSplitChange.emit($event)"
      />
      <ng-template #focusedDocTpl>
        @if (focusedDoc(); as entry) {
          <ng-container
            [ngTemplateOutlet]="documentPaneTemplate()"
            [ngTemplateOutletContext]="{ $implicit: entry }"
          />
        }
      </ng-template>
    } @else {
      <div class="qt-doc-split-layout">
        <div class="qt-doc-chat-pane" style="flex: 1; min-width: 0">
          <ng-container [ngTemplateOutlet]="chatContent()" />
        </div>
      </div>
      <!-- The VCR the pane embedded views are created into (their nodes are then
           moved out to the child-tab hosts); the holder parks a pane whose host
           has not registered yet. Both stay display:none. -->
      <ng-container #paneAnchor />
      <div #holder class="qt-salon-portal-holder" style="display: none"></div>
    }
  `,
})
export class SalonModePanes {
  private readonly workspace = inject(WORKSPACE_HANDLE, { optional: true });
  private readonly parentTabId = inject(WORKSPACE_TAB_ID, { optional: true });
  private readonly registry = inject(WORKSPACE_PORTAL_REGISTRY, { optional: true });
  private readonly destroyRef = inject(DestroyRef);
  private readonly paneAnchor = viewChild('paneAnchor', { read: ViewContainerRef });
  private readonly holder = viewChild<ElementRef<HTMLElement>>('holder');

  readonly parentChatId = input.required<string>();
  readonly chatTitle = input<string | null>(null);
  readonly mode = input.required<'normal' | 'split' | 'focus'>();
  readonly dividerPosition = input.required<number>();
  readonly rightPaneVerticalSplit = input.required<number>();
  readonly chatContent = input.required<TemplateRef<unknown>>();
  /** Instantiated once per open document with the entry as `$implicit`. */
  readonly documentPaneTemplate = input.required<TemplateRef<{ $implicit: OpenDocEntry }>>();
  readonly documentEntries = input<OpenDocEntry[]>([]);
  readonly focusedDocId = input<string | null>(null);
  readonly terminalContent = input<TemplateRef<unknown> | null>(null);
  readonly terminalActive = input<boolean>(false);

  readonly closeDocument = output<string>();
  readonly closeTerminal = output<void>();
  readonly dividerPositionChange = output<number>();
  readonly rightPaneVerticalSplitChange = output<number>();

  protected readonly inWorkspace = computed(
    () => !!(this.workspace && this.parentTabId != null && this.registry),
  );

  /** The focused document — the one the legacy single-pane route shows (v4 `activeDocument`). */
  protected readonly focusedDoc = computed<OpenDocEntry | null>(() => {
    const id = this.focusedDocId();
    const entries = this.documentEntries();
    return entries.find((e) => e.document.id === id) ?? entries[0] ?? null;
  });

  protected docPortalKey(docId: string): string {
    return portalKey('document', this.parentChatId(), docId);
  }
  protected terminalPortalKey(): string {
    return portalKey('terminal', this.parentChatId());
  }

  /** docId → the child tab id this view opened. */
  private readonly docTabs = new Map<string, string>();
  private termTab: string | null = null;
  /** Portal keys whose child-tab host has been seen registered — a seen key
   * with no node means the child tab was CLOSED (the reverse-close signal). */
  private readonly seenPortalKeys = new Set<string>();
  private destroyed = false;
  /** docId → its live portaled pane; plus the single terminal pane. */
  private readonly docPanes = new Map<string, PortaledPane>();
  private termPane: PortaledPane | null = null;

  constructor() {
    // Reconcile the Document child tabs with the open-document set (v4
    // SalonModePanes effect #1/#3 — minus the state-poll arm the reduced handle
    // can't support; that's reducer-side at unify).
    effect(() => {
      if (!this.inWorkspace()) return;
      const ws = this.workspace!;
      const parentTabId = this.parentTabId!;
      const entries = this.documentEntries();
      const openIds = new Set(entries.map((e) => e.document.id));
      for (const entry of entries) {
        if (this.docTabs.has(entry.document.id)) continue;
        const title =
          entry.document.displayTitle ||
          (this.chatTitle() ? `Document: ${this.chatTitle()}` : 'Document');
        const tabId = ws.openTab(
          'document',
          {
            chatId: this.parentChatId(),
            chatDocumentId: entry.document.id,
            displayTitle: entry.document.displayTitle,
          },
          { parentTabId, title },
        );
        this.docTabs.set(entry.document.id, tabId);
      }
      for (const [docId, tabId] of [...this.docTabs]) {
        if (!openIds.has(docId)) {
          ws.closeTab(tabId);
          this.docTabs.delete(docId);
          this.seenPortalKeys.delete(this.docPortalKey(docId));
        }
      }
    });

    // Reconcile the Terminal child tab with terminal-mode state (v4 effect #2).
    effect(() => {
      if (!this.inWorkspace()) return;
      const ws = this.workspace!;
      const parentTabId = this.parentTabId!;
      if (this.terminalActive()) {
        if (!this.termTab) {
          this.termTab = ws.openTab(
            'terminal',
            { chatId: this.parentChatId() },
            { parentTabId, title: this.chatTitle() ? `Terminal: ${this.chatTitle()}` : 'Terminal' },
          );
        }
      } else if (this.termTab) {
        ws.closeTab(this.termTab);
        this.termTab = null;
        this.seenPortalKeys.delete(this.terminalPortalKey());
      }
    });

    // Create/destroy the per-document embedded-view panes (workspace only).
    effect(() => {
      if (!this.inWorkspace()) {
        this.teardownDocPanes();
        return;
      }
      const vcr = this.paneAnchor();
      const holder = this.holder()?.nativeElement;
      const tpl = this.documentPaneTemplate();
      if (!vcr || !holder) return;
      const entries = this.documentEntries();
      const openIds = new Set(entries.map((e) => e.document.id));
      for (const entry of entries) {
        if (this.docPanes.has(entry.document.id)) continue;
        this.docPanes.set(
          entry.document.id,
          this.createPane(vcr, holder, tpl, { $implicit: entry }, this.docPortalKey(entry.document.id)),
        );
      }
      for (const [docId, pane] of [...this.docPanes]) {
        if (!openIds.has(docId)) {
          this.destroyPane(pane);
          this.docPanes.delete(docId);
        }
      }
    });

    // Create/destroy the terminal embedded-view pane (workspace only).
    effect(() => {
      if (!this.inWorkspace()) {
        if (this.termPane) {
          this.destroyPane(this.termPane);
          this.termPane = null;
        }
        return;
      }
      const vcr = this.paneAnchor();
      const holder = this.holder()?.nativeElement;
      const tpl = this.terminalContent();
      if (!vcr || !holder) return;
      if (tpl && !this.termPane) {
        this.termPane = this.createPane(vcr, holder, tpl, undefined, this.terminalPortalKey());
      } else if (!tpl && this.termPane) {
        this.destroyPane(this.termPane);
        this.termPane = null;
      }
    });

    // Relocate each live pane wrapper into its registered child-tab node (v4's
    // createPortal target). Re-runs on registry changes AND on the pane set.
    effect(() => {
      if (!this.inWorkspace() || !this.registry) return;
      const nodes = this.registry.nodes();
      this.documentEntries();
      this.terminalContent();
      const panes = [...this.docPanes.values(), ...(this.termPane ? [this.termPane] : [])];
      for (const pane of panes) {
        const key = pane.wrapper.getAttribute('data-portal-key');
        if (!key) continue;
        const target = nodes[key] ?? null;
        if (target && pane.wrapper.parentElement !== target) target.appendChild(pane.wrapper);
      }
    });

    // The REVERSE close direction (the p4.9j unification wire): react to a
    // previously-registered portal node disappearing — the child tab was
    // closed — and close the document / toggle the terminal off. Guarded by
    // `destroyed` so the teardown path (which also unregisters nodes via the
    // hosts' own destroy) can never fire spurious server-visible closes.
    effect(() => {
      if (!this.inWorkspace() || !this.registry || this.destroyed) return;
      const nodes = this.registry.nodes();
      for (const docId of [...this.docTabs.keys()]) {
        const key = this.docPortalKey(docId);
        if (nodes[key]) this.seenPortalKeys.add(key);
        else if (this.seenPortalKeys.has(key)) {
          this.seenPortalKeys.delete(key);
          this.docTabs.delete(docId);
          this.closeDocument.emit(docId);
        }
      }
      if (this.termTab) {
        const termKey = this.terminalPortalKey();
        if (nodes[termKey]) this.seenPortalKeys.add(termKey);
        else if (this.seenPortalKeys.has(termKey)) {
          this.seenPortalKeys.delete(termKey);
          this.termTab = null;
          this.closeTerminal.emit();
        }
      }
    });

    this.destroyRef.onDestroy(() => {
      this.destroyed = true;
      this.teardownDocPanes();
      if (this.termPane) {
        this.destroyPane(this.termPane);
        this.termPane = null;
      }
      const ws = this.workspace;
      if (ws) {
        for (const tabId of this.docTabs.values()) ws.closeTab(tabId);
        this.docTabs.clear();
        if (this.termTab) {
          ws.closeTab(this.termTab);
          this.termTab = null;
        }
      }
    });
  }

  /** Create an embedded view, move its root nodes into a portaled wrapper. */
  private createPane(
    vcr: ViewContainerRef,
    holder: HTMLElement,
    tpl: TemplateRef<unknown>,
    context: unknown,
    key: string,
  ): PortaledPane {
    const view = vcr.createEmbeddedView(tpl, context) as EmbeddedViewRef<unknown>;
    view.detectChanges();
    const wrapper = document.createElement('div');
    wrapper.className = 'qt-salon-portaled-pane';
    wrapper.setAttribute('data-portal-key', key);
    for (const node of view.rootNodes as Node[]) wrapper.appendChild(node);
    holder.appendChild(wrapper);
    return { view, wrapper };
  }

  private destroyPane(pane: PortaledPane): void {
    pane.view.destroy();
    pane.wrapper.remove();
  }

  private teardownDocPanes(): void {
    for (const pane of this.docPanes.values()) this.destroyPane(pane);
    this.docPanes.clear();
  }
}
