import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it } from 'vitest';

import {
  DEFAULT_ROUTE,
  provideDocumentStoreTesting,
  settle,
  store,
  type Route,
} from './document-browser.testing';
import { ProjectStoreBrowser } from './project-store-browser';

/**
 * v4 `ProjectStoreBrowser.tsx` (`c17033f04`, pin `624d0d25c`; P4.D387 item
 * 13): the official store (C14) wins, the primary linked one otherwise, and
 * v4's three texts byte-exact — note the STRAIGHT apostrophe in "Finding the
 * project's shelves…" (v4 writes `&apos;`) and the CURLY one in "The
 * project’s document store could not be reached just now."
 */

@Component({
  imports: [ProjectStoreBrowser],
  template: `<qt-project-store-browser [projectId]="projectId()" />`,
})
class Host {
  readonly projectId = signal('p1');
}

async function render(route: Route) {
  const { seen } = provideDocumentStoreTesting(route, [provideRouter([])]);
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, seen };
}

const linked = [
  { ...store('store-x', 'Elsewhere'), mountType: 'filesystem' },
  { ...store('store-a', 'Archive') },
];

describe('ProjectStoreBrowser (v4 ProjectStoreBrowser.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('says it is finding the shelves, then browses the official store', async () => {
    const { fixture, el, seen } = await render((req) =>
      req.type === 'projectMountPointList'
        ? { mountPoints: linked, officialMountPointId: 'store-a' }
        : DEFAULT_ROUTE(req),
    );
    expect(el.textContent!.trim()).toBe("Finding the project's shelves…");
    await settle(fixture);
    expect(seen).toContainEqual({ type: 'projectMountPointList', projectId: 'p1' });
    expect(el.querySelector('qt-document-store-browser')).not.toBeNull();
    expect(seen).toContainEqual({ type: 'mountPointGet', mountPointId: 'store-a' });
  });

  it('without an official id, picks the primary linked database store', async () => {
    const { fixture, seen } = await render((req) =>
      req.type === 'projectMountPointList' ? { mountPoints: linked } : DEFAULT_ROUTE(req),
    );
    await settle(fixture);
    expect(seen).toContainEqual({ type: 'mountFilesList', mountPointId: 'store-a' });
  });

  it('says so when the project has no store of its own', async () => {
    const { fixture, el } = await render(() => ({ mountPoints: [], officialMountPointId: null }));
    await settle(fixture);
    expect(el.textContent!.trim()).toBe('This project has no document store of its own.');
  });

  it('says so when the read fails', async () => {
    const { fixture, el } = await render(() => new Error('down'));
    await settle(fixture);
    expect(el.textContent!.trim()).toBe(
      'The project’s document store could not be reached just now.',
    );
  });
});
