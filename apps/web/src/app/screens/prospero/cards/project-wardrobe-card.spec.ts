import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub } from '../../../core/core-client.testing';
import type { CoreRequest, WardrobeItemDto } from '../../../core/core-contract';
import { WardrobeBrowser } from '../../../wardrobe/wardrobe-browser';
import { WardrobeItemOverlays } from '../../../wardrobe/wardrobe-item-overlays';
import { ProjectWardrobeCard } from './project-wardrobe-card';

/**
 * P4.D285 — v4 `app/prospero/[id]/components/WardrobeCard.tsx` at
 * `4a38222e6`: the card IS the shared browser on the project's container
 * (`ProjectWardrobeManager` deleted), titled by the list's length, with the
 * dressing-instructions section above it, `scrollList={false}`, and the
 * overlays mounted outside the collapsible body labelled with the project's
 * name (`'This project'` when absent).
 */

type AnyRequest = CoreRequest & Record<string, unknown>;

const item = (id: string, extra: Partial<WardrobeItemDto> = {}): WardrobeItemDto => ({
  id,
  title: id,
  types: ['top'],
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  ...extra,
});

async function settle(fixture: ComponentFixture<unknown>, ticks = 10): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

async function render(projectName?: string, defaultOpen = true) {
  const seen: AnyRequest[] = [];
  const core = {
    ...coreStreamStub(),
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as AnyRequest;
      seen.push(r);
      switch (r.type as string) {
        case 'projectWardrobeList':
          return { wardrobeItems: [item('Livery Coat'), item('Apron', { folder: 'Kitchen' })] };
        case 'wardrobeList':
          return { wardrobeItems: [item('Watch')] };
        case 'wardrobeFoldersList':
          return { folders: [{ path: 'Kitchen', name: 'Kitchen', itemCount: 1 }] };
        default:
          return {};
      }
    }),
  } as unknown as CoreClient;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProjectWardrobeCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(ProjectWardrobeCard);
  fixture.componentRef.setInput('projectId', 'p1');
  if (projectName !== undefined) fixture.componentRef.setInput('projectName', projectName);
  fixture.componentRef.setInput('defaultOpen', defaultOpen);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, seen, el: fixture.nativeElement as HTMLElement };
}

describe('ProjectWardrobeCard (v4 `WardrobeCard.tsx`)', () => {
  it('mounts the browser only when open: a collapsed card reads no folders until opened (E.1)', async () => {
    const { el, fixture, seen } = await render('The Estate', false);
    expect(fixture.debugElement.query(By.directive(WardrobeBrowser))).toBeNull();
    expect(seen.some((r) => r.type === 'wardrobeFoldersList')).toBe(false);
    const header = el.querySelector('qt-collapsible-card button') as HTMLButtonElement;
    header.click();
    await settle(fixture);
    expect(fixture.debugElement.query(By.directive(WardrobeBrowser))).not.toBeNull();
    expect(seen.some((r) => r.type === 'wardrobeFoldersList')).toBe(true);
  });

  it('titles the card by the project list and mounts the shared browser on the project', async () => {
    const { el, fixture, seen } = await render('The Estate');
    expect(el.textContent).toContain('Wardrobe (2)');
    expect(el.textContent).toContain('Shared garments every character in this project can wear');
    const browser = fixture.debugElement.query(By.directive(WardrobeBrowser));
    expect(browser).not.toBeNull();
    const b = browser.componentInstance as WardrobeBrowser;
    expect(b.container()).toEqual({ scope: 'project', id: 'p1' });
    expect(b.scrollList()).toBe(false);
    expect(seen.some((r) => r.type === 'projectWardrobeList' && r['projectId'] === 'p1')).toBe(true);
    expect(seen.some((r) => r.type === 'wardrobeFoldersList' && r['containerId'] === 'p1')).toBe(true);
    // The folder row, then the root's own item; no tier rows in a project.
    expect(el.querySelectorAll('[data-testid="wardrobe-folder-row"]')).toHaveLength(1);
    expect(el.querySelector('[title="Livery Coat"]')).not.toBeNull();
    expect(el.querySelector('[title="Apron"]')).toBeNull();
    expect(el.querySelectorAll('[data-testid="wardrobe-tier-row"]')).toHaveLength(0);
    // The capped list is the dialog's; the card lets the list grow.
    const list = el.querySelector('qt-wardrobe-browser section > div.flex-1') as HTMLElement;
    expect(list.classList.contains('overflow-y-auto')).toBe(false);
  });

  it('mounts the dressing instructions above the browser, and the overlays outside the card body', async () => {
    const { el, fixture } = await render('The Estate');
    const instructions = el.querySelector('qt-wardrobe-instructions-section')!;
    const browser = el.querySelector('qt-wardrobe-browser')!;
    expect(instructions.compareDocumentPosition(browser) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const overlays = fixture.debugElement.query(By.directive(WardrobeItemOverlays));
    expect(el.querySelector('qt-collapsible-card')!.contains(overlays.nativeElement)).toBe(false);
    expect((overlays.componentInstance as WardrobeItemOverlays).containerLabel()).toBe('The Estate');
  });

  it('labels the overlays "This project" when the name is unknown', async () => {
    const { fixture } = await render();
    const overlays = fixture.debugElement.query(By.directive(WardrobeItemOverlays));
    expect((overlays.componentInstance as WardrobeItemOverlays).containerLabel()).toBe('This project');
  });
});
