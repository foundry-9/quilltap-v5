import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub } from '../../../core/core-client.testing';
import type { CoreRequest, ScopedEvent } from '../../../core/core-contract';
import { LAMP, SWORD } from '../../../inventory/__fixtures__/inventory-ui.fixture';
import { ProjectInventoryCard } from './project-inventory-card';

/**
 * P4.D330 — v4 `app/prospero/[id]/components/InventoryCard.tsx` (`4fbdad186`,
 * `e27fb6b34`'s icon): `Inventory (n)` from the project tier's own list, v4's
 * description, the `inventory` icon, and the pinned panel (no select).
 */

async function settle(fixture: ComponentFixture<unknown>, ticks = 10): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(defaultOpen: boolean) {
  const seen: Array<Record<string, unknown>> = [];
  const stream = coreStreamStub();
  const core = {
    ...stream,
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as unknown as Record<string, unknown>;
      seen.push(r);
      switch (r['type']) {
        case 'inventoryList':
          return { items: [SWORD, LAMP] };
        case 'inventoryFolders':
          return { folders: [] };
        default:
          return {};
      }
    }),
  } as unknown as CoreClient;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProjectInventoryCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(ProjectInventoryCard);
  fixture.componentRef.setInput('projectId', 'p-1');
  fixture.componentRef.setInput('projectName', 'The Quest');
  fixture.componentRef.setInput('defaultOpen', defaultOpen);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, el: fixture.nativeElement as HTMLElement, seen, stream };
}

describe('ProjectInventoryCard (v4 InventoryCard.tsx)', () => {
  it('titles itself by the project tier’s count, with v4’s description and the inventory icon', async () => {
    const { el, seen } = await render(false);
    expect(el.textContent).toContain('Inventory (2)');
    expect(el.textContent).toContain('Objects, kits and archetypes every character in this project can carry');
    expect(el.querySelector('qt-icon span')!.getAttribute('data-icon')).toBe('inventory');
    expect(seen).toContainEqual({ type: 'inventoryList', container: { scope: 'project', id: 'p-1' } });
  });

  it('opened, it shows the panel pinned to the project — no container select', async () => {
    const { el } = await render(true);
    expect(el.querySelector('qt-inventory-panel')).not.toBeNull();
    expect(el.querySelector('#inventory-container-select')).toBeNull();
    expect(el.querySelectorAll('[data-testid="inventory-item-row"]').length).toBe(2);
  });

  it('the inventory realtime topic re-counts', async () => {
    const { fixture, seen, stream } = await render(false);
    const before = seen.filter((r) => r['type'] === 'inventoryList').length;
    stream.frames.next({ v: 1, topic: 'inventory', at: 1 } as unknown as ScopedEvent);
    await settle(fixture);
    expect(seen.filter((r) => r['type'] === 'inventoryList').length).toBeGreaterThan(before);
  });
});
