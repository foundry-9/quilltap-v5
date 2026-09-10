import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { coreStreamStub } from '../../core/core-client.testing';
import { OrganizeSection } from './organize-section';

/** The Organize drawer's entries and their gate (v4 `OrganizeSection`). */

type Req = { type: string; [k: string]: unknown };

function stubClient(
  galleryTotal: number | 'error' = 'error',
  seen: Req[] = [],
): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: async (req: Req) => {
      seen.push(req);
      if (req.type === 'chatGallery') {
        if (galleryTotal === 'error') throw new Error("unknown variant `chatGallery`");
        return {
          entries: [],
          counts: {
            'story-background': 0,
            avatar: 0,
            portrait: 0,
            generated: 0,
            attachment: 0,
            kept: 0,
            inline: 0,
          },
          total: galleryTotal,
        };
      }
      return {};
    },
  } as unknown as CoreClient;
}

@Component({
  imports: [OrganizeSection],
  template: `
    <qt-organize-section
      [chatId]="'chat-1'"
      [isAutonomousRoom]="isAutonomousRoom()"
      (editEnclave)="fired.push('enclave')"
      (rename)="fired.push('rename')"
      (mergeIn)="fired.push('merge')"
      (openState)="fired.push('state')"
      (openGallery)="fired.push('gallery')"
    />
  `,
})
class Host {
  readonly isAutonomousRoom = signal(false);
  readonly fired: string[] = [];
}

async function render(core: CoreClient = stubClient()): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: core },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function labels(fixture: ComponentFixture<Host>): string[] {
  return Array.from(fixture.nativeElement.querySelectorAll('button')).map((b) =>
    (b as HTMLButtonElement).textContent!.trim(),
  );
}

describe('OrganizeSection', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows Copy ID, State and Gallery (numbered from the first paint — v4 ChatSidebar.tsx:1676) — and Edit Enclave only for an autonomous room', async () => {
    const fixture = await render();
    expect(labels(fixture)).toEqual([
      'Copy ID',
      'Rename',
      'State…',
      'Merge In…',
      'Export',
      'Export Markdown',
      'Gallery (0)',
    ]);

    fixture.componentInstance.isAutonomousRoom.set(true);
    fixture.detectChanges();
    // Merge In… is HIDDEN in an autonomous room (v4 :1566 `&& !isAutonomousRoom`).
    expect(labels(fixture)).toEqual([
      'Edit Enclave',
      'Copy ID',
      'Rename',
      'State…',
      'Export',
      'Export Markdown',
      'Gallery (0)',
    ]);
  });

  it('reports each entry to the Salon', async () => {
    const fixture = await render();
    fixture.componentInstance.isAutonomousRoom.set(true);
    fixture.detectChanges();

    for (const label of ['Edit Enclave', 'Rename', 'State…', 'Gallery (0)']) {
      const button = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
        (b) => (b as HTMLButtonElement).textContent!.trim() === label,
      ) as HTMLButtonElement;
      button.click();
    }
    expect(fixture.componentInstance.fired).toEqual(['enclave', 'rename', 'state', 'gallery']);
  });

  it('downloads the Markdown transcript by anchor-click, as v4 does', async () => {
    const fixture = await render();
    // `triggerUrlDownload` appends a real anchor and clicks it; intercept the
    // click so the jsdom navigation never happens, and read the href off the
    // element the helper built.
    const clicked: Array<{ href: string; download: string }> = [];
    const spy = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(function (this: HTMLAnchorElement) {
        clicked.push({ href: this.href, download: this.download });
      });

    const button = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent!.trim() === 'Export Markdown',
    ) as HTMLButtonElement;
    button.click();

    expect(spy).toHaveBeenCalledOnce();
    expect(clicked[0].href).toContain('/api/v1/chats/chat-1?action=export-markdown');
    // Only the anchor's fallback name — the server's Content-Disposition names
    // the file.
    expect(clicked[0].download).toBe('chat_transcript.md');
    // The entry must not fire a Salon output (v4 keeps it entirely local).
    expect(fixture.componentInstance.fired).toEqual([]);
  });
  /**
   * v4 4.8.2's export fix: the JSONL entry used to set `window.location.href`,
   * which downloads in a browser but NAVIGATES the app window onto the API
   * route in a native shell (v4's Electron, v5's Tauri `qtap://` webview). It
   * now anchor-clicks through the same helper the Markdown entry uses.
   */
  it('downloads the .qtap export by anchor-click, never by navigating (v4 4.8.2)', async () => {
    const fixture = await render();
    const clicked: Array<{ href: string; download: string }> = [];
    const spy = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(function (this: HTMLAnchorElement) {
        clicked.push({ href: this.href, download: this.download });
      });
    const hrefBefore = window.location.href;

    const button = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent!.trim() === 'Export',
    ) as HTMLButtonElement;
    button.click();

    expect(spy).toHaveBeenCalledOnce();
    expect(clicked[0].href).toContain('/api/v1/chats/chat-1?action=export');
    expect(clicked[0].href).not.toContain('export-markdown');
    // Only the anchor's fallback name — Content-Disposition names the real file.
    expect(clicked[0].download).toBe('chat_export.qtap');
    // Nothing navigated.
    expect(window.location.href).toBe(hrefBefore);
    expect(fixture.componentInstance.fired).toEqual([]);
  });

  /**
   * P4.D176 — the retired divergence. The label's number comes from the
   * `chatGallery` query, and ONLY from it: a mocked server that does not yet
   * implement the verb (v4's pre-P4.D174 shape in-lane) renders the entry
   * unnumbered rather than crashing or inventing a count.
   */
  describe('the Gallery count (bug 129 — the invariant carried, not the shape)', () => {
    it('reads the label number from chatGallery.total, not from any chat-read field', async () => {
      const seen: Req[] = [];
      const fixture = await render(stubClient(7, seen));
      const gallery = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
        (b) => (b as HTMLButtonElement).getAttribute('title') === 'Every image in this conversation',
      ) as HTMLButtonElement;
      expect(gallery.textContent!.trim()).toBe('Gallery (7)');
      // The count's fetch reached a verb that EXISTS (the bug-129 invariant).
      expect(seen.some((r) => r.type === 'chatGallery' && r['chatId'] === 'chat-1')).toBe(true);
    });

    it('a chat with genuinely no pictures reads (0), not unnumbered', async () => {
      const fixture = await render(stubClient(0));
      expect(labels(fixture)).toContain('Gallery (0)');
    });

    it('a FAILED chatGallery read still renders the entry, numbered 0 — v4 ChatSidebar.tsx:1563 defaults galleryCount to 0, never gates', async () => {
      const fixture = await render(stubClient('error'));
      // The entry still renders — v4's ungated post-bug-129 shape — with v4's
      // default count rather than a bare label (the §3 unification review of
      // the `78b381a96` round retired the bare `Gallery` fallback).
      expect(labels(fixture)).toContain('Gallery (0)');
    });
  });
});
