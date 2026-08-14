import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { OrganizeSection } from './organize-section';

/** The Organize drawer's entries and their gate (v4 `OrganizeSection`). */

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

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
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

  it('shows Copy ID, State and Gallery — and Edit Enclave only for an autonomous room', async () => {
    const fixture = await render();
    expect(labels(fixture)).toEqual([
      'Copy ID',
      'Rename',
      'State…',
      'Merge In…',
      'Export',
      'Export Markdown',
      'Gallery',
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
      'Gallery',
    ]);
  });

  it('reports each entry to the Salon', async () => {
    const fixture = await render();
    fixture.componentInstance.isAutonomousRoom.set(true);
    fixture.detectChanges();

    for (const label of ['Edit Enclave', 'Rename', 'State…', 'Gallery']) {
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
    // the file after the chat.
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
});
