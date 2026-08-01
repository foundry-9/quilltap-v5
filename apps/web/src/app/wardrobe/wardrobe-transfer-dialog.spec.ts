import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import type { TransferDestinationsPayload } from './wardrobe.api';
import {
  decodeDestination,
  encodeDestination,
  WardrobeTransferDialog,
} from './wardrobe-transfer-dialog';
import { ToastService } from '../ui/toast.service';

type AnyRequest = CoreRequest & Record<string, unknown>;

const ITEM: WardrobeItemDto = {
  id: 'w1',
  characterId: 'char-1',
  title: 'Charcoal Sweater',
  types: ['top'],
  componentItemIds: [],
  isDefault: false,
  replace: false,
  createdAt: '2024-01-01T00:00:00.000Z',
  updatedAt: '2024-01-01T00:00:00.000Z',
} as WardrobeItemDto;

const DESTINATIONS: TransferDestinationsPayload = {
  general: { available: true, label: 'Quilltap General' },
  projects: [{ id: 'p1', name: 'The Estate' }],
  groups: [],
  users: [{ id: 'u1', name: 'Riya' }],
};

async function settle(fixture: ComponentFixture<unknown>, ticks = 4): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

async function render(
  mode: 'move' | 'copy',
  route: (req: AnyRequest) => Record<string, unknown> | Error = () => ({
    destinations: DESTINATIONS,
  }),
): Promise<{ fixture: ComponentFixture<WardrobeTransferDialog>; seen: AnyRequest[] }> {
  const seen: AnyRequest[] = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as AnyRequest;
      seen.push(r);
      const out = route(r);
      if (out instanceof Error) throw out;
      return out;
    }),
  } as unknown as CoreClient;

  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeTransferDialog],
    providers: [{ provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(WardrobeTransferDialog);
  fixture.componentRef.setInput('mode', mode);
  fixture.componentRef.setInput('item', ITEM);
  fixture.componentRef.setInput('sourceCharacterId', 'char-1');
  fixture.componentRef.setInput('sourceProjectId', 'p-src');
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, seen };
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('destination encoding (v4 WardrobeTransferDialog.tsx:38-50)', () => {
  it('round-trips scope:id and rejects unknown scopes', () => {
    expect(encodeDestination('general', null)).toBe('general:');
    expect(decodeDestination('project:p1')).toEqual({ scope: 'project', id: 'p1' });
    expect(decodeDestination('general:')).toEqual({ scope: 'general', id: null });
    expect(decodeDestination('bogus:x')).toBeNull();
    expect(decodeDestination('')).toBeNull();
  });
});

describe('WardrobeTransferDialog (v4 WardrobeTransferDialog.tsx)', () => {
  it('loads destinations on open and preselects General when available (v4 :73-83)', async () => {
    const { fixture, seen } = await render('move');
    expect(seen[0]).toEqual({ type: 'wardrobeTransferDestinations' });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Move wardrobe item');
    expect(text).toContain('Quilltap General');
    expect(text).toContain('The Estate');
    expect(text).toContain('Riya');
    const general = (fixture.nativeElement as HTMLElement).querySelector(
      'option[value="general:"]',
    ) as HTMLOptionElement;
    expect(general.selected).toBe(true);
  });

  it('submits v4\'s POST body verbatim — id omitted for general (v4 :107-116) — and emits transferred', async () => {
    const { fixture, seen } = await render('copy');
    const transferredSpy = vi.fn();
    fixture.componentInstance.transferred.subscribe(transferredSpy);
    const submit = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').trim() === 'Copy item') as HTMLButtonElement;
    submit.click();
    await settle(fixture);
    expect(seen.at(-1)).toEqual({
      type: 'wardrobeTransferApply',
      action: 'copy',
      itemId: 'w1',
      sourceCharacterId: 'char-1',
      sourceProjectId: 'p-src',
      destination: { scope: 'general' },
    });
    expect(transferredSpy).toHaveBeenCalledTimes(1);
  });

  it('a scoped destination rides its id (v4 destination.id spread)', async () => {
    const { fixture, seen } = await render('move');
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      'select',
    ) as HTMLSelectElement;
    select.value = 'character:u1';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    const submit = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').trim() === 'Move item') as HTMLButtonElement;
    submit.click();
    await settle(fixture);
    expect(seen.at(-1)).toMatchObject({
      type: 'wardrobeTransferApply',
      action: 'move',
      destination: { scope: 'character', id: 'u1' },
    });
  });

  it('a failed load surfaces the error as v4\'s toast', async () => {
    await render('move', () => new Error('unknown request type'));
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'unknown request type' });
  });
});
