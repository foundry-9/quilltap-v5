import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { QueryClient } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_ROUTE,
  provideDocumentStoreTesting,
  settle,
} from '../../document-browser/document-browser.testing';
import { mountPointKeys } from '../../document-browser/document-store.keys';
import { WORKSPACE_TAB_ID, WORKSPACE_TAB_VISIBLE } from '../../workspace/workspace-contract';
import { StoreDetail } from './store-detail';

/**
 * The store detail page on v4 `c34986421`'s view (P4.D387 item 14): the
 * section heading is `Files` (v4 `DocumentStoreDetailView.tsx:207`) and the
 * body the document-store browser for the store (`:208`); the record is read
 * through the browser's own key (`mountPointKeys.detail`, v4
 * `useDocumentStoreDetail`); a "where else" jump into another store opens it —
 * in the tab by `openStore` (v4 `ScriptoriumView.tsx:146`), on the routed page
 * through the router (v4 `DocumentStoreDetailPageClient.tsx:21`); a tab
 * re-activation invalidates the store's reads (v4's `refresh()`).
 *
 * RE-AIMED (D18 retirement, ruling R-D): the file-manager beta toggle case
 * this file held (its v4 `:238-244` title pin) has no counterpart after
 * `c34986421` — the toggle is gone from v4 — and is removed.
 */

async function render(opts: { embedded: boolean; visible?: ReturnType<typeof signal<boolean>> }) {
  const { seen, queryClient } = provideDocumentStoreTesting(DEFAULT_ROUTE, [
    provideRouter([]),
    { provide: WORKSPACE_TAB_ID, useValue: opts.visible ? 'tab-s' : null },
    { provide: WORKSPACE_TAB_VISIBLE, useValue: opts.visible ?? (() => true) },
  ]);
  const fixture: ComponentFixture<StoreDetail> = TestBed.createComponent(StoreDetail);
  if (opts.embedded) fixture.componentRef.setInput('storeId', 'store-a');
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, seen, queryClient, el: fixture.nativeElement as HTMLElement };
}

describe('StoreDetail — the Files section (v4 DocumentStoreDetailView.tsx:203-209 at c34986421)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('heads the section Files and mounts the browser for the store', async () => {
    const { el, seen } = await render({ embedded: true });
    const headings = [...el.querySelectorAll('h2')].map((h) => h.textContent!.trim());
    expect(headings).toContain('Files');
    expect(headings).not.toContain('Indexed Files');
    expect(el.querySelector('qt-document-store-browser')).not.toBeNull();
    expect(seen).toContainEqual({ type: 'mountFilesList', mountPointId: 'store-a' });
    // The page and the browser read the record under ONE key
    // (`mountPointKeys.detail`); the browser's observer mounting later
    // refetches the stale entry, as react-query does in v4.
    expect(
      seen.filter((r) => r.type === 'mountPointGet').every((r) => r['mountPointId'] === 'store-a'),
    ).toBe(true);
  });

  it('in the tab, another store opens through openStore (the list drills to it)', async () => {
    const { fixture } = await render({ embedded: true });
    const opened: string[] = [];
    fixture.componentInstance.openStore.subscribe((id) => opened.push(id));
    (fixture.componentInstance as unknown as { openOtherStore(id: string): void }).openOtherStore(
      'store-b',
    );
    expect(opened).toEqual(['store-b']);
  });

  it('on the routed page, another store opens through the router', async () => {
    const { fixture } = await render({ embedded: false });
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    (fixture.componentInstance as unknown as { openOtherStore(id: string): void }).openOtherStore(
      'store-b',
    );
    expect(navigate).toHaveBeenCalledWith(['/scriptorium', 'store-b']);
  });

  it("a tab re-activation invalidates the store's reads and the store lists (v4 refresh)", async () => {
    const visible = signal(true);
    const { fixture, queryClient } = await render({ embedded: true, visible });
    const invalidate = vi.spyOn(TestBed.inject(QueryClient), 'invalidateQueries');
    expect(queryClient).toBe(TestBed.inject(QueryClient));
    visible.set(false);
    fixture.detectChanges();
    visible.set(true);
    fixture.detectChanges();
    await settle(fixture);
    const keys = invalidate.mock.calls.map((c) =>
      JSON.stringify((c[0] as { queryKey: unknown }).queryKey),
    );
    expect(keys).toContain(JSON.stringify(mountPointKeys.store('store-a')));
    expect(keys).toContain(JSON.stringify(mountPointKeys.lists));
  });
});
