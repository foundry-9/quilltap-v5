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

interface RenderOpts {
  item?: WardrobeItemDto;
  sourceCharacterId?: string | null;
  source?: { scope: string; id: string | null } | null;
  excludeDestination?: { scope: string; id: string | null } | null;
}

async function render(
  mode: 'move' | 'copy',
  route: (req: AnyRequest) => Record<string, unknown> | Error = () => ({
    destinations: DESTINATIONS,
  }),
  opts: RenderOpts = {},
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
  fixture.componentRef.setInput('item', opts.item ?? ITEM);
  fixture.componentRef.setInput(
    'sourceCharacterId',
    opts.sourceCharacterId === undefined ? 'char-1' : opts.sourceCharacterId,
  );
  fixture.componentRef.setInput('sourceProjectId', 'p-src');
  if (opts.source !== undefined) fixture.componentRef.setInput('source', opts.source);
  if (opts.excludeDestination !== undefined) {
    fixture.componentRef.setInput('excludeDestination', opts.excludeDestination);
  }
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

// ---------------------------------------------------------------------------
// The known-home exclusion and the component prompt (v4 `d7263f39` + `f6a10055`)
// ---------------------------------------------------------------------------

const OUTFIT: WardrobeItemDto = {
  ...ITEM,
  id: 'w-outfit',
  title: 'Evening Kit',
  componentItemIds: ['a', 'b'],
} as WardrobeItemDto;

function options(fixture: ComponentFixture<WardrobeTransferDialog>): string[] {
  return [
    ...(fixture.nativeElement as HTMLElement).querySelectorAll('select option'),
  ].map((o) => (o as HTMLOptionElement).value);
}

function radios(fixture: ComponentFixture<WardrobeTransferDialog>): string[] {
  return [
    ...(fixture.nativeElement as HTMLElement).querySelectorAll(
      'input[name="wardrobe-transfer-components"]',
    ),
  ].map((r) => ((r as HTMLInputElement).closest('label')?.textContent ?? '').trim());
}

async function submit(
  fixture: ComponentFixture<WardrobeTransferDialog>,
  label: string,
): Promise<void> {
  const button = [
    ...(fixture.nativeElement as HTMLElement).querySelectorAll('button'),
  ].find((b) => (b.textContent ?? '').trim() === label) as HTMLButtonElement;
  button.click();
  await settle(fixture);
}

describe("the item's known home is hidden from the destinations (v4 :126-148)", () => {
  it('excludes General and skips the preselect when General IS the home (v4 :101-108)', async () => {
    const { fixture } = await render('move', undefined, {
      excludeDestination: { scope: 'general', id: null },
      source: { scope: 'general', id: null },
      sourceCharacterId: null,
    });
    expect(options(fixture)).toEqual(['project:p1', 'character:u1']);
    // Nothing is preselected, so the submit button stays disarmed.
    const move = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ].find((b) => (b.textContent ?? '').trim() === 'Move item') as HTMLButtonElement;
    expect(move.disabled).toBe(true);
  });

  it('excludes a project, a group, or a character home by scope AND id', async () => {
    const project = await render('copy', undefined, {
      excludeDestination: { scope: 'project', id: 'p1' },
    });
    expect(options(project.fixture)).toEqual(['general:', 'character:u1']);

    const character = await render('copy', undefined, {
      excludeDestination: { scope: 'character', id: 'u1' },
    });
    expect(options(character.fixture)).toEqual(['general:', 'project:p1']);

    // A DIFFERENT id of the same scope is left alone.
    const other = await render('copy', undefined, {
      excludeDestination: { scope: 'project', id: 'p-other' },
    });
    expect(options(other.fixture)).toEqual(['general:', 'project:p1', 'character:u1']);
  });

  it('with every destination excluded the placeholder appears (v4 :227-233)', async () => {
    const { fixture } = await render(
      'move',
      () => ({
        destinations: {
          general: { available: true, label: 'Quilltap General' },
          projects: [],
          groups: [],
          users: [],
        },
      }),
      { excludeDestination: { scope: 'general', id: null } },
    );
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'No destinations available',
    );
  });
});

describe('the component prompt for a composite outfit (v4 f6a10055 :290-327)', () => {
  it('a non-composite item shows no prompt and sends no `components` key', async () => {
    const { fixture, seen } = await render('move');
    expect(radios(fixture)).toEqual([]);
    await submit(fixture, 'Move item');
    expect(seen.at(-1)).not.toHaveProperty('components');
  });

  it('moving an outfit offers move / copy / leave, defaulting to move (v4 :92-94)', async () => {
    const { fixture, seen } = await render('move', undefined, { item: OUTFIT });
    expect(radios(fixture)).toEqual([
      'Move the components along with it',
      'Copy the components (originals stay behind)',
      'Leave the components behind',
    ]);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'This outfit bundles 2 components',
    );
    await submit(fixture, 'Move item');
    expect(seen.at(-1)).toMatchObject({ components: 'move' });
  });

  it('copying an outfit offers copy / don\'t — MOVE is unreachable, which is the refusal (v4 refine)', async () => {
    const { fixture, seen } = await render('copy', undefined, { item: OUTFIT });
    expect(radios(fixture)).toEqual([
      'Copy the components along with it',
      'Copy the outfit alone',
    ]);
    await submit(fixture, 'Copy item');
    expect(seen.at(-1)).toMatchObject({ components: 'copy' });
  });

  it('the chosen mode reaches the wire (v4 :168)', async () => {
    const { fixture, seen } = await render('move', undefined, { item: OUTFIT });
    const leave = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll(
        'input[name="wardrobe-transfer-components"]',
      ),
    ][2] as HTMLInputElement;
    leave.click();
    await settle(fixture);
    await submit(fixture, 'Move item');
    expect(seen.at(-1)).toMatchObject({ components: 'none' });
  });

  it('a single-component outfit says "component", not "components"', async () => {
    const { fixture } = await render('move', undefined, {
      item: { ...OUTFIT, componentItemIds: ['a'] } as WardrobeItemDto,
    });
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'This outfit bundles 1 component',
    );
  });

  it("a server refusal surfaces as v4's error toast", async () => {
    const { fixture } = await render(
      'copy',
      (req) =>
        (req.type as string) === 'wardrobeTransferApply'
          ? new Error(
              'Copying an outfit cannot move its components — the original outfit still needs them',
            )
          : { destinations: DESTINATIONS },
      { item: OUTFIT },
    );
    await submit(fixture, 'Copy item');
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message:
        'Copying an outfit cannot move its components — the original outfit still needs them',
    });
  });
});

describe('the explicit source container on the wire (v4 :163-168)', () => {
  it('browsing a shared container sends `source` and OMITS sourceCharacterId', async () => {
    const { fixture, seen } = await render('move', undefined, {
      sourceCharacterId: null,
      source: { scope: 'group', id: 'g1' },
    });
    await submit(fixture, 'Move item');
    const body = seen.at(-1) as Record<string, unknown>;
    expect(body).toMatchObject({ source: { scope: 'group', id: 'g1' } });
    // The key is ABSENT, not null — the server's refine reads presence.
    expect(body).not.toHaveProperty('sourceCharacterId');
  });

  it('the General source carries no id (v4 `...(source.id ? {id} : {})`)', async () => {
    const { fixture, seen } = await render('move', undefined, {
      sourceCharacterId: null,
      source: { scope: 'general', id: null },
      excludeDestination: { scope: 'general', id: null },
    });
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      'select',
    ) as HTMLSelectElement;
    select.value = 'project:p1';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    await submit(fixture, 'Move item');
    expect((seen.at(-1) as unknown as { source: Record<string, unknown> }).source).toEqual({
      scope: 'general',
    });
  });

  it('the character view sends sourceCharacterId and NO source (v4 :1481)', async () => {
    const { fixture, seen } = await render('move');
    await submit(fixture, 'Move item');
    const body = seen.at(-1) as Record<string, unknown>;
    expect(body).toMatchObject({ sourceCharacterId: 'char-1' });
    expect(body).not.toHaveProperty('source');
  });
});
