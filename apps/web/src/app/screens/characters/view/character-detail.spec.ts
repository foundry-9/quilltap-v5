import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter, Router } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { of } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterDetail as CharacterDetailDto } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import {
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
  type WorkspaceHandle,
} from '../../../workspace/workspace-contract';
import { CharacterDetail } from './character-detail';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function character(over: Partial<CharacterDetailDto> = {}): CharacterDetailDto {
  return {
    id: 'c1',
    name: 'Bertie',
    title: 'The Valet',
    identity: 'A gentleman of leisure.',
    description: null,
    manifesto: null,
    personality: null,
    scenarios: [],
    firstMessage: null,
    exampleDialogues: null,
    systemPrompts: [],
    physicalDescription: null,
    defaultImageId: null,
    defaultImage: null,
    defaultConnectionProfileId: null,
    controlledBy: 'llm',
    isFavorite: false,
    canBeCarina: false,
    npc: false,
    defaultAgentModeEnabled: null,
    defaultHelpToolsEnabled: null,
    canDressThemselves: null,
    canCreateOutfits: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultPartnerId: null,
    defaultImageProfileId: null,
    aliases: [],
    pronouns: null,
    characterDocumentMountPointId: null,
    tags: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    _count: { chats: 0 },
    ...over,
  };
}

function stubClient(
  c: CharacterDetailDto,
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  let current = c;
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      switch (req.type) {
        case 'characterGet':
          return { character: current };
        case 'connectionProfileList':
          return { profiles: [] };
        case 'characterList':
          return { characters: [] };
        case 'characterDefaultPartner':
          return { partnerId: null };
        case 'characterStats':
          return {
            stats: {
              memories: 0,
              conversations: 0,
              wardrobeItems: 0,
              photos: 0,
              scenarios: 0,
              knowledge: 0,
              core: 0,
              characterFiles: 0,
              characterFilesTotal: 0,
            },
            groups: [],
          };
        case 'characterFavorite':
          current = { ...current, isFavorite: !current.isFavorite };
          return { character: current };
        case 'characterToggleCarina':
          current = { ...current, canBeCarina: !current.canBeCarina };
          return { character: current };
        case 'characterToggleControlledBy':
          current = { ...current, controlledBy: current.controlledBy === 'user' ? 'llm' : 'user' };
          return { character: current };
        case 'characterUpdate':
          return { character: current };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterDetail>> {
  TestBed.configureTestingModule({
    imports: [CharacterDetail],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
      {
        provide: ActivatedRoute,
        useValue: {
          paramMap: of(convertToParamMap({ id: 'c1' })),
          queryParamMap: of(convertToParamMap({})),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(CharacterDetail);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('CharacterDetail', () => {
  it('loads the character and renders the header + the nine-tab set', async () => {
    const fixture = await render(stubClient(character({})));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Bertie');
    expect(text).toContain('The Valet');
    expect(text).toContain('← Back to Characters');
    for (const label of [
      'Details',
      'System Prompts',
      'Conversations',
      'Memories',
      'Tags',
      'Wardrobe',
      'Default Settings',
      'Photo Gallery',
      'Appearance',
    ]) {
      expect(text).toContain(label);
    }
  });

  it('dispatches characterFavorite when the favorite star is toggled', async () => {
    const seen: string[] = [];
    const fixture = await render(
      stubClient(character({ isFavorite: false }), (req) => seen.push(req.type)),
    );
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const favorite = buttons.find((b) => b.title === 'Add to favorites');
    expect(favorite).toBeTruthy();
    favorite!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen).toContain('characterFavorite');
  });

  it('dispatches characterToggleCarina when the Carina toggle is clicked', async () => {
    const seen: string[] = [];
    const fixture = await render(
      stubClient(character({ canBeCarina: false }), (req) => seen.push(req.type)),
    );
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const carina = buttons.find((b) => b.title === 'Enable Carina answers (@-queries)');
    expect(carina).toBeTruthy();
    carina!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen).toContain('characterToggleCarina');
  });

  it('dispatches characterToggleControlledBy when the controlled-by toggle is clicked', async () => {
    const seen: string[] = [];
    const fixture = await render(
      stubClient(character({ controlledBy: 'llm' }), (req) => seen.push(req.type)),
    );
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const controlledBy = buttons.find((b) => b.title === 'Switch to user control');
    expect(controlledBy).toBeTruthy();
    controlledBy!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen).toContain('characterToggleControlledBy');
  });

  it('dispatches characterUpdate with npc:true when Convert to NPC is clicked', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient(character({ npc: false }), (req) => seen.push(req)));
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const convert = buttons.find((b) => b.textContent?.trim() === 'Convert to NPC');
    expect(convert).toBeTruthy();
    convert!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const update = seen.find((r) => r.type === 'characterUpdate');
    expect(update).toBeTruthy();
    expect(update!['character']).toEqual({ npc: true });
  });
});

/** v4 `useCharacterView.ts:606-680` — the toggle toasts the list screen already
 *  covers; this pins the SAME arms on the detail screen's own header. */
describe('CharacterDetail toasts', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function failingStub(failType: string, message?: string): Partial<CoreClient> {
    const base = stubClient(character({}));
    return {
      dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
        if (req.type === failType) {
          throw message ? new Error(message) : new Error();
        }
        return (base.dispatchData as (r: typeof req) => Promise<unknown>)(req);
      }) as CoreClient['dispatchData'],
    };
  }

  function button(fixture: ComponentFixture<CharacterDetail>, title: string): HTMLButtonElement {
    return [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.title === title,
    ) as HTMLButtonElement;
  }

  it("toasts v4's sentence when the favorite toggle fails", async () => {
    const fixture = await render(failingStub('characterFavorite', 'Failed to toggle favorite'));
    button(fixture, 'Add to favorites').click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to toggle favorite' }]);
  });

  it("toasts v4's sentence when the Carina toggle fails", async () => {
    const fixture = await render(
      failingStub('characterToggleCarina', 'Failed to toggle Carina eligibility'),
    );
    button(fixture, 'Enable Carina answers (@-queries)').click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to toggle Carina eligibility' }]);
  });

  it("toasts v4's sentence when the controlled-by toggle fails", async () => {
    const fixture = await render(
      failingStub('characterToggleControlledBy', 'Failed to toggle controlled-by'),
    );
    button(fixture, 'Switch to user control').click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to toggle controlled-by' }]);
  });

  it('toasts "Converted to NPC" / "Converted to Character" on success', async () => {
    const fixture = await render(stubClient(character({ npc: false })));
    const convert = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Convert to NPC',
    ) as HTMLButtonElement;
    convert.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'success', message: 'Converted to NPC' }]);
  });

  it('reverts and toasts on a failed NPC toggle', async () => {
    const fixture = await render(failingStub('characterUpdate', 'the ledger will not budge'));
    const convert = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Convert to NPC',
    ) as HTMLButtonElement;
    convert.click();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(toasts()).toEqual([{ type: 'error', message: 'the ledger will not budge' }]);
    expect(fixture.nativeElement.textContent).toContain('Convert to NPC');
  });
});

/**
 * The start-chat auto-open (p4.9j3 item 2, v4 `CharacterDetailView` `:136-143`).
 * v5 DIVERGENCE: no in-place NewChatModal — the guard navigates to the
 * established `/salon/new?characterId=` entry, ONCE, after the character loads,
 * from either arm (`openChatOnMount` input or routed `?action=chat`).
 */
describe('CharacterDetail (start-chat auto-open guard)', () => {
  async function renderWithNavSpy(opts: {
    openChatOnMount?: boolean;
    action?: string;
    tabMode?: boolean;
  }): Promise<ReturnType<typeof vi.fn>> {
    TestBed.configureTestingModule({
      imports: [CharacterDetail],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: stubClient(character({})) },
        ...(opts.tabMode
          ? []
          : [
              {
                provide: ActivatedRoute,
                useValue: {
                  paramMap: of(convertToParamMap({ id: 'c1' })),
                  queryParamMap: of(convertToParamMap(opts.action ? { action: opts.action } : {})),
                },
              },
            ]),
      ],
    });
    const navigate = vi.fn(async () => true);
    vi.spyOn(TestBed.inject(Router), 'navigate').mockImplementation(navigate as never);
    const fixture = TestBed.createComponent(CharacterDetail);
    if (opts.tabMode) fixture.componentRef.setInput('characterId', 'c1');
    if (opts.openChatOnMount) fixture.componentRef.setInput('openChatOnMount', true);
    fixture.detectChanges();
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return navigate;
  }

  it('openChatOnMount navigates ONCE to /salon/new?characterId= after load (the in-tab drill arm)', async () => {
    const navigate = await renderWithNavSpy({ openChatOnMount: true, tabMode: true });
    expect(navigate).toHaveBeenCalledTimes(1);
    expect(navigate).toHaveBeenCalledWith(['/salon/new'], { queryParams: { characterId: 'c1' } });
  });

  it('the routed ?action=chat arm navigates to /salon/new (v4 page.tsx:21)', async () => {
    const navigate = await renderWithNavSpy({ action: 'chat' });
    expect(navigate).toHaveBeenCalledWith(['/salon/new'], { queryParams: { characterId: 'c1' } });
  });

  it('does NOT auto-open without the flag or the ?action=chat param', async () => {
    const navigate = await renderWithNavSpy({});
    expect(navigate).not.toHaveBeenCalled();
  });
});

/**
 * Workspace-tab mode (p4.9j2, v4 `CharacterViewTab`): the identity comes from
 * the `characterId` input (NO `ActivatedRoute`), `tab` deep-links a sub-tab, and
 * "back" closes the tab via the handle instead of routing.
 */
describe('CharacterDetail (workspace-tab mode)', () => {
  function handle(): { handle: WorkspaceHandle; closed: string[] } {
    const closed: string[] = [];
    return {
      closed,
      handle: {
        openTab: vi.fn(() => 'x'),
        closeTab: vi.fn((id: string) => closed.push(id)),
        refreshTab: vi.fn(),
      },
    };
  }

  async function render(
    client: Partial<CoreClient>,
    h: WorkspaceHandle,
    tab?: string,
  ): Promise<ComponentFixture<CharacterDetail>> {
    TestBed.configureTestingModule({
      imports: [CharacterDetail],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
        { provide: WORKSPACE_HANDLE, useValue: h },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-9' },
      ],
    });
    const fixture = TestBed.createComponent(CharacterDetail);
    fixture.componentRef.setInput('characterId', 'c1');
    if (tab) fixture.componentRef.setInput('tab', tab);
    fixture.detectChanges();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it('loads the character from the characterId input with no ActivatedRoute', async () => {
    const { handle: h } = handle();
    const fixture = await render(stubClient(character({})), h);
    expect(fixture.nativeElement.textContent).toContain('Bertie');
  });

  it('renders "back" as a button that closes the tab (v4 CharacterViewTab)', async () => {
    const { handle: h, closed } = handle();
    const fixture = await render(stubClient(character({})), h);
    const back = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      (b as HTMLButtonElement).textContent?.includes('Back to Characters'),
    ) as HTMLButtonElement;
    expect(back).toBeTruthy();
    // No routerLink anchor in tab mode.
    expect(fixture.nativeElement.querySelector('a[href="/characters"]')).toBeNull();
    back.click();
    expect(closed).toEqual(['tab-9']);
  });

  it('deep-links a sub-tab from the tab input', async () => {
    const { handle: h } = handle();
    const fixture = await render(stubClient(character({})), h, 'conversations');
    const active = fixture.nativeElement.querySelector('.qt-tab-active') as HTMLElement;
    expect(active.textContent).toContain('Conversations');
  });
});

// ===========================================================================
// P4.D64 — the archived character's page (v4 `CharacterDetailView.tsx` +
// `CharacterHeader.tsx` + `CharacterDetails.tsx` at `d553f72a`)
// ===========================================================================

const ARCHIVED_AT = '2026-08-01T00:00:00.000Z';

/** A stub whose character is archived, with archive/rehydrate replies pluggable. */
function archiveClient(opts: {
  archivedAt?: string | null;
  onArchive?: () => Promise<Record<string, unknown>>;
  onRehydrate?: () => Promise<Record<string, unknown>>;
  seen?: Array<Record<string, unknown>>;
}): Partial<CoreClient> {
  // `??` would swallow an explicit null (the live-character arm), so read the
  // key's PRESENCE, not its truthiness.
  let current = character({
    archivedAt: 'archivedAt' in opts ? opts.archivedAt : ARCHIVED_AT,
  });
  return {
    dispatchData: (async (req: Record<string, unknown>) => {
      opts.seen?.push(req);
      switch (req['type']) {
        case 'characterGet':
          return { character: current };
        case 'characterArchive': {
          const body = (await opts.onArchive?.()) ?? {
            archived: true,
            archiveFileId: 'bundle-1',
            pruneComplete: true,
          };
          current = { ...current, archivedAt: ARCHIVED_AT };
          return body;
        }
        case 'characterRehydrate': {
          const body = (await opts.onRehydrate?.()) ?? {
            rehydrated: true,
            archived: false,
            archiveBundleFileId: null,
            warnings: [],
          };
          current = { ...current, archivedAt: null };
          return body;
        }
        case 'connectionProfileList':
          return { profiles: [] };
        case 'characterList':
          return { characters: [] };
        case 'characterDefaultPartner':
          return { partnerId: null };
        case 'characterStats':
          return {
            stats: {
              memories: 0,
              conversations: 0,
              wardrobeItems: 0,
              photos: 0,
              scenarios: 0,
              knowledge: 0,
              core: 0,
              characterFiles: 0,
              characterFilesTotal: 0,
            },
            groups: [],
          };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  };
}

function findButton(fixture: ComponentFixture<CharacterDetail>, label: string): HTMLButtonElement {
  const found = (
    Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
  ).find((b) => b.textContent?.trim() === label);
  expect(found, `button "${label}"`).toBeTruthy();
  return found!;
}

/**
 * A button INSIDE a named dialog. The header's own "Archive" is still on the
 * page while the confirm dialog is open, so an unscoped label lookup finds the
 * opener and silently re-opens instead of confirming.
 */
function inDialog(
  fixture: ComponentFixture<CharacterDetail>,
  selector: string,
  label: string,
): HTMLButtonElement {
  const host = fixture.nativeElement.querySelector(selector) as HTMLElement | null;
  expect(host, selector).toBeTruthy();
  const found = (Array.from(host!.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent?.trim() === label,
  );
  expect(found, `${selector} button "${label}"`).toBeTruthy();
  return found!;
}

async function settle(fixture: ComponentFixture<CharacterDetail>): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('CharacterDetail — the archived read-only page (P4.D64)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the archived banner above the header, verbatim', async () => {
    const fixture = await render(archiveClient({}));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Bertie rests in the archive.');
    expect(text).toContain(
      'Their effects — memories, correspondence, photographs, summaries — are packed into a sealed bundle on the shelf.',
    );
    expect(text).toContain(
      'rehydrate them to pick up the pen again. Old conversations keep their words and their face throughout.',
    );
  });

  it('shows no banner for a live character', async () => {
    const fixture = await render(stubClient(character({})));
    expect(fixture.nativeElement.textContent).not.toContain('rests in the archive');
  });

  it('wraps the tab content in a disabled fieldset — the courtesy over the write guard', async () => {
    const fixture = await render(archiveClient({}));
    const fieldset = fixture.nativeElement.querySelector('fieldset') as HTMLFieldSetElement;
    expect(fieldset).toBeTruthy();
    expect(fieldset.disabled).toBe(true);
    expect(fieldset.className).toContain('opacity-90');
    // The tab body still RENDERS inside it — archived is readable, not blank.
    expect(fieldset.querySelector('qt-character-details-tab')).toBeTruthy();
  });

  it('leaves a live character unfieldsetted', async () => {
    const fixture = await render(stubClient(character({})));
    expect(fixture.nativeElement.querySelector('fieldset')).toBeNull();
    expect(fixture.nativeElement.querySelector('qt-character-details-tab')).toBeTruthy();
  });

  it('hides the Edit Character door (a disabled fieldset cannot inert a link)', async () => {
    const fixture = await render(archiveClient({}));
    expect(fixture.nativeElement.textContent).not.toContain('Edit Character');
  });

  it('keeps the Edit Character door for a live character', async () => {
    const fixture = await render(stubClient(character({})));
    expect(fixture.nativeElement.textContent).toContain('Edit Character');
  });

  it('forks the header: Rehydrate only, no chat / conversion / toggles', async () => {
    const fixture = await render(archiveClient({}));
    const text = fixture.nativeElement.textContent as string;
    const rehydrate = findButton(fixture, 'Rehydrate');
    expect(rehydrate.getAttribute('title')).toBe(
      'Restore this character from their archive bundle and wake them',
    );
    expect(text).not.toContain('Start Chat');
    expect(text).not.toContain('Convert to NPC');
    // No Archive BUTTON — asserted structurally, since the name badge legitimately
    // reads "Archived" and a substring check would pass for the wrong reason.
    const labels = (
      Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
    ).map((b) => b.textContent?.trim());
    expect(labels).not.toContain('Archive');
    // The favorite / Carina / control cluster is gone from the header too.
    expect(text).not.toContain('☆');
  });

  it('badges the header name with a dated Archived chip', async () => {
    const fixture = await render(archiveClient({}));
    const badge = (
      Array.from(fixture.nativeElement.querySelectorAll('h1 span')) as HTMLElement[]
    ).find((s) => s.textContent?.trim() === 'Archived');
    expect(badge).toBeTruthy();
    expect(badge!.getAttribute('title')).toBe(
      `Resting in the archive since ${new Date(ARCHIVED_AT).toLocaleDateString()}`,
    );
  });

  it('offers Archive LAST in the live cluster with the d69287d9-fixed classes', async () => {
    const fixture = await render(stubClient(character({})));
    const archive = findButton(fixture, 'Archive');
    expect(archive.getAttribute('title')).toBe(
      "Pack this character's effects into a sealed bundle and set them resting in the archive",
    );
    // d69287d9: `text-foreground`, matching its siblings — NOT the muted token
    // disabled controls use, which made the one live action read as unavailable.
    expect(archive.className).toContain('text-foreground');
    expect(archive.className).not.toContain('qt-text-secondary');
    // Last in the action column, after the three disabled deferrals.
    const column = archive.parentElement as HTMLElement;
    expect(column.lastElementChild).toBe(archive);
    expect(fixture.nativeElement.textContent).not.toContain('Rehydrate');
  });
});

describe('CharacterDetail — the archive dialog + its toasts (P4.D64)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function openArchiveDialog(
    client: Partial<CoreClient>,
  ): Promise<ComponentFixture<CharacterDetail>> {
    const fixture = await render(client);
    findButton(fixture, 'Archive').click();
    await settle(fixture);
    return fixture;
  }

  it('spells out what is packed and what is kept, verbatim', async () => {
    const fixture = await openArchiveDialog(stubClient(character({})));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Set Bertie resting in the archive?');
    expect(text).toContain(
      'Archiving packs the whole of Bertie — every last letter and photograph — into a single sealed bundle on the archive shelf, then clears the heavier effects from the working rooms. Nothing is lost; it is merely put away, and rehydrating unpacks it all again.',
    );
    expect(text).toContain('Packed into the bundle and cleared away:');
    for (const item of [
      'Their memories (the Commonplace Book falls silent)',
      'Their correspondence — the whole of the mail folder',
      'Every photograph beyond the portrait itself',
      'Their conversation summaries',
    ]) {
      expect(text).toContain(item);
    }
    expect(text).toContain('Kept in place, exactly as it stands:');
    for (const item of [
      'Who they are — every character field, still readable on their page',
      'Their portrait, so old conversations keep their face',
      'Their wardrobe',
      'Every chat they took part in, word for word',
      "archiving silences the character, not everyone's memory of them",
    ]) {
      expect(text).toContain(item);
    }
    expect(text).toContain(
      'While archived they take no turns, receive no letters, and answer no queries. The bundle is sealed with your passphrase and rests at',
    );
    expect(text).toContain('files/<id>/character-archive.qtap');
    expect(text).toContain('Leave Them Be');
  });

  it('emphasizes "other" in the memory-of-them line (v4 markup, not just prose)', async () => {
    const fixture = await openArchiveDialog(stubClient(character({})));
    const em = fixture.nativeElement.querySelector('qt-archive-character-dialog em') as HTMLElement;
    expect(em?.textContent?.trim()).toBe('other');
  });

  it('archives, closes, and toasts the clean-sweep sentence', async () => {
    const seen: Array<Record<string, unknown>> = [];
    const fixture = await openArchiveDialog(
      archiveClient({ archivedAt: null, seen, onArchive: async () => ({ archived: true, archiveFileId: 'b1', pruneComplete: true }) }),
    );
    inDialog(fixture, 'qt-archive-character-dialog', 'Archive').click();
    await settle(fixture);
    expect(seen.some((r) => r['type'] === 'characterArchive' && r['characterId'] === 'c1')).toBe(true);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Bertie rests in the archive, bundle sealed and shelved.' },
    ]);
    expect(fixture.nativeElement.querySelector('qt-archive-character-dialog')).toBeNull();
  });

  it('toasts the resisted-packing sentence when pruneComplete is FALSE', async () => {
    // Only an explicit `false` takes this arm (v4 `:286` `=== false`), which is
    // why the default reply above — and a body missing the key — reads as clean.
    const fixture = await openArchiveDialog(
      archiveClient({
        archivedAt: null,
        onArchive: async () => ({ archived: true, archiveFileId: 'b1', pruneComplete: false }),
      }),
    );
    inDialog(fixture, 'qt-archive-character-dialog', 'Archive').click();
    await settle(fixture);
    expect(toasts()).toEqual([
      {
        type: 'success',
        message:
          'Bertie is archived, but some effects resisted packing — archive them again to finish the sweep.',
      },
    ]);
  });

  it('reads a body with NO pruneComplete key as a clean sweep (v4 === false)', async () => {
    // The strictness is load-bearing: `!pruneComplete` would turn every body
    // that omits the key into the alarming "some effects resisted packing"
    // sentence. v4 compares `=== false`.
    const fixture = await openArchiveDialog(
      archiveClient({
        archivedAt: null,
        onArchive: async () => ({ archived: true, archiveFileId: 'b1' }),
      }),
    );
    inDialog(fixture, 'qt-archive-character-dialog', 'Archive').click();
    await settle(fixture);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Bertie rests in the archive, bundle sealed and shelved.' },
    ]);
  });

  it('keeps the dialog open and toasts the server sentence on failure', async () => {
    const fixture = await openArchiveDialog(
      archiveClient({
        archivedAt: null,
        onArchive: async () => {
          throw new Error('the archive cannot be sealed: your passphrase has not been entered');
        },
      }),
    );
    inDialog(fixture, 'qt-archive-character-dialog', 'Archive').click();
    await settle(fixture);
    expect(toasts()).toEqual([
      {
        type: 'error',
        message: 'the archive cannot be sealed: your passphrase has not been entered',
      },
    ]);
    expect(fixture.nativeElement.querySelector('qt-archive-character-dialog')).toBeTruthy();
  });

  it('Leave Them Be closes without dispatching', async () => {
    const seen: Array<Record<string, unknown>> = [];
    const fixture = await openArchiveDialog(archiveClient({ archivedAt: null, seen }));
    inDialog(fixture, 'qt-archive-character-dialog', 'Leave Them Be').click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-archive-character-dialog')).toBeNull();
    expect(seen.some((r) => r['type'] === 'characterArchive')).toBe(false);
  });
});

describe('CharacterDetail — rehydrate + the leftover bundle (P4.D64)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('toasts the unpacked counts and offers the bundle when one is left', async () => {
    const fixture = await render(
      archiveClient({
        onRehydrate: async () => ({
          rehydrated: true,
          archived: false,
          archiveBundleFileId: 'bundle-9',
          restored: { memories: 42, documents: 3, blobs: 7 },
          warnings: [],
        }),
      }),
    );
    findButton(fixture, 'Rehydrate').click();
    await settle(fixture);
    expect(toasts()).toEqual([
      {
        type: 'success',
        message: 'Bertie is awake again — 42 memories, 3 papers and 7 photographs unpacked.',
      },
    ]);
    const dialog = fixture.nativeElement.querySelector('qt-rehydrate-bundle-dialog') as HTMLElement;
    expect(dialog).toBeTruthy();
    const text = dialog.textContent as string;
    expect(text).toContain('The empty bundle remains on the shelf');
    expect(text).toContain(
      'Bertie is fully unpacked — memories, correspondence and photographs all back where they belong. The sealed bundle they travelled in still sits in the file library, and keeping it costs nothing but shelf space: it is a spare copy of everything the archive held, exactly as it was.',
    );
    expect(text).toContain(
      'Discard it and the spare copy is gone for good — though of course you can always archive Bertie afresh, which packs a new bundle from their current state.',
    );
  });

  it('toasts the bare sentence and offers no dialog when nothing is left behind', async () => {
    const fixture = await render(
      archiveClient({
        onRehydrate: async () => ({
          rehydrated: true,
          archived: false,
          // v4 `:314` requires a NON-EMPTY string: '' must not open the dialog.
          archiveBundleFileId: '',
          warnings: [],
        }),
      }),
    );
    findButton(fixture, 'Rehydrate').click();
    await settle(fixture);
    expect(toasts()).toEqual([{ type: 'success', message: 'Bertie is awake again.' }]);
    expect(fixture.nativeElement.querySelector('qt-rehydrate-bundle-dialog')).toBeNull();
  });

  it('keeps the destructive arm SECONDARY and Keep It primary (v4 polarity)', async () => {
    const fixture = await render(
      archiveClient({
        onRehydrate: async () => ({
          rehydrated: true,
          archived: false,
          archiveBundleFileId: 'bundle-9',
          warnings: [],
        }),
      }),
    );
    findButton(fixture, 'Rehydrate').click();
    await settle(fixture);
    const discard = findButton(fixture, 'Discard the Bundle');
    const keep = findButton(fixture, 'Keep It');
    // The polarity is deliberate: discarding is the only irreversible half.
    expect(discard.className).toContain('qt-button-secondary');
    expect(keep.className).toContain('qt-button-primary');
  });

  it('Discard deletes the file and toasts, Keep It just closes', async () => {
    const seen: Array<Record<string, unknown>> = [];
    const client = archiveClient({
      seen,
      onRehydrate: async () => ({
        rehydrated: true,
        archived: false,
        archiveBundleFileId: 'bundle-9',
        warnings: [],
      }),
    });
    const fixture = await render(client);
    findButton(fixture, 'Rehydrate').click();
    await settle(fixture);
    inDialog(fixture, 'qt-rehydrate-bundle-dialog', 'Discard the Bundle').click();
    await settle(fixture);
    expect(seen.some((r) => r['type'] === 'fileDelete' && r['fileId'] === 'bundle-9')).toBe(true);
    expect(toasts().map((t) => t.message)).toContain('The bundle is off the shelf.');
    expect(fixture.nativeElement.querySelector('qt-rehydrate-bundle-dialog')).toBeNull();
  });

  it('toasts the server sentence when rehydration fails and stays archived', async () => {
    const fixture = await render(
      archiveClient({
        onRehydrate: async () => {
          throw new Error('the bundle is missing from the shelf');
        },
      }),
    );
    findButton(fixture, 'Rehydrate').click();
    await settle(fixture);
    expect(toasts()).toEqual([{ type: 'error', message: 'the bundle is missing from the shelf' }]);
    expect(fixture.nativeElement.textContent).toContain('rests in the archive');
  });
});
