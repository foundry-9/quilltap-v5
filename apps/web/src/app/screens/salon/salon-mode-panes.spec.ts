import { Component, computed, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { OpenDocEntry } from '../../documents/document-mode';
import {
  WORKSPACE_HANDLE,
  WORKSPACE_PORTAL_REGISTRY,
  WORKSPACE_TAB_ID,
  portalKey,
  type WorkspaceHandle,
  type WorkspacePortalRegistry,
} from '../../workspace/workspace-contract';
import { SalonModePanes } from './salon-mode-panes';

/**
 * Port of v4 `__tests__/unit/components/workspace/salon-mode-panes.test.tsx`,
 * case-for-case: the legacy fallback (no workspace → in-chat SplitLayout, one
 * focused document) and the workspace branch (each open document spawns its own
 * child tab and the pane is relocated into that tab's registered host), plus a
 * keep-alive spec proving the pane ELEMENT survives the DOM move (§3).
 */

function entry(id: string, over: Partial<OpenDocEntry['document']> = {}): OpenDocEntry {
  return {
    document: {
      id,
      displayTitle: id.toUpperCase(),
      filePath: `${id}.md`,
      content: '',
      mtime: 1,
      ...over,
    },
  } as OpenDocEntry;
}

@Component({
  imports: [SalonModePanes],
  template: `
    <ng-template #chat><div>CHAT-BODY</div></ng-template>
    <ng-template #docPane let-e><div class="doc-body">DOC-{{ e.document.id }}</div></ng-template>
    <ng-template #term><div class="term-body">TERM-BODY</div></ng-template>
    <qt-salon-mode-panes
      [parentChatId]="'c1'"
      [chatTitle]="'My Chat'"
      [mode]="mode()"
      [dividerPosition]="50"
      [rightPaneVerticalSplit]="50"
      [chatContent]="chat"
      [documentPaneTemplate]="docPane"
      [documentEntries]="entries()"
      [focusedDocId]="focusedDocId()"
      [terminalContent]="terminalActive() ? term : null"
      [terminalActive]="terminalActive()"
      (closeDocument)="closeDoc($event)"
      (closeTerminal)="terminalActive.set(false)"
    />
    <div #hostD1 data-testid="host-d1"></div>
    <div #hostD2 data-testid="host-d2"></div>
  `,
})
class Host {
  readonly mode = signal<'normal' | 'split' | 'focus'>('normal');
  readonly entries = signal<OpenDocEntry[]>([]);
  readonly focusedDocId = computed(() => this.entries().at(-1)?.document.id ?? null);
  readonly terminalActive = signal(false);

  openDoc(id: string): void {
    this.entries.update((prev) =>
      prev.some((e) => e.document.id === id) ? prev : [...prev, entry(id)],
    );
  }
  closeDoc(id: string): void {
    this.entries.update((prev) => prev.filter((e) => e.document.id !== id));
  }
}

async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function fakeWorkspace(): {
  handle: WorkspaceHandle;
  docTabCount: () => number;
  termTabCount: () => number;
} {
  const tabs = new Map<string, { kind: string }>();
  let counter = 0;
  return {
    handle: {
      openTab: (kind) => {
        const id = `tab-${++counter}`;
        tabs.set(id, { kind });
        return id;
      },
      closeTab: (id) => {
        tabs.delete(id);
      },
      refreshTab: () => {},
    },
    docTabCount: () => [...tabs.values()].filter((t) => t.kind === 'document').length,
    termTabCount: () => [...tabs.values()].filter((t) => t.kind === 'terminal').length,
  };
}

function fakeRegistry(): {
  registry: WorkspacePortalRegistry;
  set: (key: string, node: HTMLElement | null) => void;
} {
  const nodes = signal<Readonly<Record<string, HTMLElement | null>>>({});
  return {
    registry: {
      setNode: (key, node) => nodes.update((n) => ({ ...n, [key]: node })),
      nodes: nodes.asReadonly(),
    },
    set: (key, node) => nodes.update((n) => ({ ...n, [key]: node })),
  };
}

describe('SalonModePanes — legacy (no workspace)', () => {
  async function render(): Promise<ComponentFixture<Host>> {
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    await settle(fixture);
    return fixture;
  }

  it('renders the in-chat SplitLayout with the chat content', async () => {
    const fixture = await render();
    expect(fixture.nativeElement.textContent).toContain('CHAT-BODY');
    expect(fixture.nativeElement.querySelector('qt-split-layout')).not.toBeNull();
  });

  it('shows the focused document in the single pane', async () => {
    const fixture = await render();
    fixture.componentInstance.mode.set('split');
    fixture.componentInstance.entries.set([entry('d1'), entry('d2')]);
    // focusedDocId derives to the last (d2).
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('DOC-d2');
    expect(fixture.nativeElement.textContent).not.toContain('DOC-d1');
  });
});

describe('SalonModePanes — workspace branch', () => {
  async function render(): Promise<{
    fixture: ComponentFixture<Host>;
    ws: ReturnType<typeof fakeWorkspace>;
    reg: ReturnType<typeof fakeRegistry>;
  }> {
    const ws = fakeWorkspace();
    const reg = fakeRegistry();
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        { provide: WORKSPACE_HANDLE, useValue: ws.handle },
        { provide: WORKSPACE_TAB_ID, useValue: 'salon-tab' },
        { provide: WORKSPACE_PORTAL_REGISTRY, useValue: reg.registry },
      ],
    });
    const fixture = TestBed.createComponent(Host);
    await settle(fixture);
    return { fixture, ws, reg };
  }

  function hostEl(fixture: ComponentFixture<Host>, testid: string): HTMLElement {
    return fixture.nativeElement.querySelector(`[data-testid="${testid}"]`) as HTMLElement;
  }

  it('opens a document child tab per open document and portals each pane into its host', async () => {
    const { fixture, ws, reg } = await render();
    // Chat content renders inline in the workspace branch.
    expect(fixture.nativeElement.textContent).toContain('CHAT-BODY');
    expect(ws.docTabCount()).toBe(0);

    // Register the two document hosts.
    reg.set(portalKey('document', 'c1', 'd1'), hostEl(fixture, 'host-d1'));
    reg.set(portalKey('document', 'c1', 'd2'), hostEl(fixture, 'host-d2'));

    fixture.componentInstance.openDoc('d1');
    await settle(fixture);
    expect(ws.docTabCount()).toBe(1);
    expect(hostEl(fixture, 'host-d1').textContent).toContain('DOC-d1');

    fixture.componentInstance.openDoc('d2');
    await settle(fixture);
    expect(ws.docTabCount()).toBe(2);
    expect(hostEl(fixture, 'host-d2').textContent).toContain('DOC-d2');
  });

  it('closes only the affected document tab when one document closes', async () => {
    const { fixture, ws, reg } = await render();
    reg.set(portalKey('document', 'c1', 'd1'), hostEl(fixture, 'host-d1'));
    reg.set(portalKey('document', 'c1', 'd2'), hostEl(fixture, 'host-d2'));

    fixture.componentInstance.openDoc('d1');
    fixture.componentInstance.openDoc('d2');
    await settle(fixture);
    expect(ws.docTabCount()).toBe(2);

    fixture.componentInstance.closeDoc('d1');
    await settle(fixture);
    expect(ws.docTabCount()).toBe(1);
    // The surviving document's pane is still portaled.
    expect(hostEl(fixture, 'host-d2').textContent).toContain('DOC-d2');
  });

  it('spawns one terminal child tab while the terminal is active', async () => {
    const { fixture, ws } = await render();
    expect(ws.termTabCount()).toBe(0);
    fixture.componentInstance.terminalActive.set(true);
    await settle(fixture);
    expect(ws.termTabCount()).toBe(1);
    fixture.componentInstance.terminalActive.set(false);
    await settle(fixture);
    expect(ws.termTabCount()).toBe(0);
  });

  it('keeps the SAME pane element alive across the DOM move (xterm/editor survive)', async () => {
    const { fixture, reg } = await render();
    reg.set(portalKey('document', 'c1', 'd1'), hostEl(fixture, 'host-d1'));
    fixture.componentInstance.openDoc('d1');
    await settle(fixture);

    const pane = hostEl(fixture, 'host-d1').querySelector('.qt-salon-portaled-pane') as HTMLElement;
    expect(pane).not.toBeNull();
    // Stamp live state onto the moved element.
    (pane as unknown as { __live: string }).__live = 'pty-attached';

    // A subsequent registry update re-runs the relocation effect; the element is
    // NOT recreated (it's already in place), so the stamped state survives.
    reg.set(portalKey('document', 'c1', 'd1'), hostEl(fixture, 'host-d1'));
    await settle(fixture);
    const still = hostEl(fixture, 'host-d1').querySelector('.qt-salon-portaled-pane') as HTMLElement;
    expect((still as unknown as { __live?: string }).__live).toBe('pty-attached');
  });

  // The REVERSE close direction (the p4.9j unification wire): the child tab's
  // portal host unregisters ONLY on tab close, so a seen-then-vanished node is
  // the close-tab signal (v4 polled `ws.state.tabs`; v5 has no tab map).
  it('closes the document when its child tab closes (portal node vanishes)', async () => {
    const { fixture, reg } = await render();
    const key = portalKey('document', 'c1', 'd1');
    reg.set(key, hostEl(fixture, 'host-d1'));
    fixture.componentInstance.openDoc('d1');
    await settle(fixture);
    expect(fixture.componentInstance.entries().length).toBe(1);

    // The user closes the child tab → its TabPortalHost unregisters the node.
    reg.set(key, null);
    await settle(fixture);
    expect(fixture.componentInstance.entries().length).toBe(0);
  });

  it('does NOT close a document whose host never registered (no false positive)', async () => {
    const { fixture } = await render();
    fixture.componentInstance.openDoc('d1');
    await settle(fixture);
    // No node was ever registered under d1's key — nothing to vanish.
    expect(fixture.componentInstance.entries().length).toBe(1);
  });

  it('toggles the terminal off when its child tab closes (portal node vanishes)', async () => {
    const { fixture, reg } = await render();
    const key = portalKey('terminal', 'c1');
    fixture.componentInstance.terminalActive.set(true);
    await settle(fixture);
    reg.set(key, hostEl(fixture, 'host-d1'));
    await settle(fixture);

    reg.set(key, null);
    await settle(fixture);
    expect(fixture.componentInstance.terminalActive()).toBe(false);
  });

  it('reopening a document after a tab-close spawns a fresh child tab (no stale close)', async () => {
    const { fixture, ws, reg } = await render();
    const key = portalKey('document', 'c1', 'd1');
    reg.set(key, hostEl(fixture, 'host-d1'));
    fixture.componentInstance.openDoc('d1');
    await settle(fixture);

    reg.set(key, null); // child tab closed → doc closes
    await settle(fixture);
    expect(fixture.componentInstance.entries().length).toBe(0);

    fixture.componentInstance.openDoc('d1'); // reopen — its host has not registered yet
    await settle(fixture);
    // The stale `seen` entry must not close the just-reopened document.
    expect(fixture.componentInstance.entries().length).toBe(1);
    expect(ws.docTabCount()).toBeGreaterThan(0);
  });
});
