import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { CoreDispatchError, type AvatarRollEntry } from '../../../../core/core-contract';
import { ToastService } from '../../../../ui/toast.service';
import { AvatarRollsSection } from './avatar-rolls-section';

/**
 * The Avatar Rolls section at v4 `AvatarRollsSection.tsx` +
 * `hooks/useAvatarRolls.ts` parity (`4dcbe0d21`). v4 ships no jest suite for
 * this family, so every case pins a transcribed behavior by `file:line`.
 */

function roll(overrides: Partial<AvatarRollEntry> = {}): AvatarRollEntry {
  return {
    fileId: 'f1',
    rollLinkId: 'rl-1',
    albumLinkId: null,
    fileName: 'plate-1.webp',
    url: '/api/v1/mount-points/mp-1/blobs/images%2Fhistory%2Fa.webp',
    mimeType: 'image/webp',
    fileSizeBytes: 2048,
    width: 512,
    height: 512,
    createdAt: '2026-09-12T10:00:00.000Z',
    generationPrompt: 'a plate',
    generationModel: 'flux',
    sha256: 'abc',
    isPortrait: false,
    usedInChatCount: 0,
    ...overrides,
  };
}

interface SeenRequest {
  type: string;
  [k: string]: unknown;
}

interface Stub {
  client: Partial<CoreClient>;
  seen: SeenRequest[];
}

function stubClient(options: {
  rolls: AvatarRollEntry[];
  total?: number;
  /** Per-verb reply override; throwing here exercises the refusal arms. */
  reply?: (req: SeenRequest) => Record<string, unknown>;
}): Stub {
  const seen: SeenRequest[] = [];
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: SeenRequest) => {
      seen.push(req);
      if (req.type === 'characterAvatarRollList') {
        return {
          entries: options.rolls,
          total: options.total ?? options.rolls.length,
          hasMore: false,
        };
      }
      if (options.reply) return options.reply(req);
      return {};
    }) as CoreClient['dispatchData'],
  };
  return { client, seen };
}

async function render(
  client: Partial<CoreClient>,
  opts: { queryClient?: QueryClient } = {},
): Promise<ComponentFixture<AvatarRollsSection>> {
  TestBed.configureTestingModule({
    imports: [AvatarRollsSection],
    providers: [
      provideTanStackQuery(opts.queryClient ?? new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(AvatarRollsSection);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.componentRef.setInput('entityName', 'Aria');
  fixture.componentRef.setInput('thumbnailSize', 120);
  fixture.detectChanges();
  await flush(fixture);
  return fixture;
}

async function flush(fixture: ComponentFixture<AvatarRollsSection>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function text(fixture: ComponentFixture<AvatarRollsSection>): string {
  return fixture.nativeElement.textContent as string;
}

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

/** Every button inside the section, by its rendered `title`. */
function buttonByTitle(
  fixture: ComponentFixture<AvatarRollsSection>,
  title: string,
): HTMLButtonElement | null {
  return fixture.nativeElement.querySelector(`button[title="${title}"]`);
}

function expandHeader(fixture: ComponentFixture<AvatarRollsSection>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('button[aria-expanded]') as HTMLButtonElement;
}

/**
 * Open the section if it is closed. IDEMPOTENT on purpose: the header is a
 * toggle, so a plain click would COLLAPSE an already-open section and make the
 * "default expanded" mutation redden every downstream case instead of the one
 * that measures the default.
 */
async function expand(fixture: ComponentFixture<AvatarRollsSection>): Promise<void> {
  if (expandHeader(fixture).getAttribute('aria-expanded') !== 'true') {
    expandHeader(fixture).click();
    await flush(fixture);
  }
}

describe('AvatarRollsSection', () => {
  afterEach(() => {
    document.body.style.overflow = '';
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  // -------------------------------------------------------------------------
  // The shell: the null arm, the header, the description
  // -------------------------------------------------------------------------

  // v4 `:117` — "say nothing at all rather than adding an empty shelf".
  it('renders NOTHING — not even the header — when the character has no rolls', async () => {
    const fixture = await render(stubClient({ rolls: [] }).client);
    expect(text(fixture).trim()).toBe('');
    expect(expandHeader(fixture)).toBeNull();
  });

  // §R.6(9): there is no empty-state string.
  it('has no empty-state sentence to show', async () => {
    const fixture = await render(stubClient({ rolls: [] }).client);
    expect(fixture.nativeElement.querySelector('section')).toBeNull();
  });

  it('renders the header once the character has a roll', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    expect(text(fixture)).toContain('Avatar Rolls');
  });

  // v4 `:130-133` — the count badge, singular and plural.
  it('counts one plate in the singular', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    expect(text(fixture)).toContain('1 plate');
    expect(text(fixture)).not.toContain('1 plates');
  });

  it('counts several plates in the plural, from `total` not the page', async () => {
    // v4 reads `data.total`, which the server may report above the 200 rows
    // the page carries.
    const fixture = await render(
      stubClient({ rolls: [roll({ fileId: 'a' }), roll({ fileId: 'b' })], total: 7 }).client,
    );
    expect(text(fixture)).toContain('7 plates');
  });

  // v4 `:132` — `…` while loading.
  it('shows an ellipsis for the count while the read is pending', () => {
    TestBed.configureTestingModule({
      imports: [AvatarRollsSection],
      providers: [
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: { dispatchData: () => new Promise(() => {}) } as Partial<CoreClient>,
        },
      ],
    });
    const fixture = TestBed.createComponent(AvatarRollsSection);
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.componentRef.setInput('entityName', 'Aria');
    fixture.componentRef.setInput('thumbnailSize', 120);
    fixture.detectChanges();
    // Assert the BADGE, not the whole section: the description paragraph
    // contains the word "plate" in every state, so a text-wide `not.toContain`
    // would be measuring the wrong span.
    const badge = fixture.nativeElement.querySelectorAll('span.qt-text-label-xs')[0] as HTMLElement;
    expect(badge.textContent!.trim()).toBe('…');
  });

  // v4 `:136-141` — the description, byte-for-byte with the name interpolated.
  it('renders the description paragraph byte-exact', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    const paragraph = (
      fixture.nativeElement.querySelector('p')!.textContent as string
    )
      .replace(/\s+/g, ' ')
      .trim();
    expect(paragraph).toBe(
      'Portraits the house has already developed for Aria — one plate per configuration of ' +
        'outfit, provider and model, kept so the same sitting need never be paid for twice. ' +
        'Keep one in the album, hang it as the portrait, take a copy away, or discard it and ' +
        'let the next sitting be drawn afresh.',
    );
  });

  // v4 `:38` — `useState(false)`.
  it('is COLLAPSED by default — no tiles until the header is clicked', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    expect(expandHeader(fixture).getAttribute('aria-expanded')).toBe('false');
    expect(fixture.nativeElement.querySelector('img')).toBeNull();
    await expand(fixture);
    expect(expandHeader(fixture).getAttribute('aria-expanded')).toBe('true');
    expect(fixture.nativeElement.querySelector('img')).not.toBeNull();
  });

  it('asks for 200 rows through the list verb', async () => {
    const stub = stubClient({ rolls: [roll()] });
    await render(stub.client);
    expect(stub.seen[0]).toEqual({
      type: 'characterAvatarRollList',
      characterId: 'c1',
      limit: 200,
    });
  });

  // -------------------------------------------------------------------------
  // The tile
  // -------------------------------------------------------------------------

  it('renders the tile at the album’s thumbnail size', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    await expand(fixture);
    const grid = fixture.nativeElement.querySelector('.grid') as HTMLElement;
    expect(grid.style.gridTemplateColumns).toContain('120px');
  });

  // v4 `:64` — the badge follows `isPortrait`, never a raw `defaultImageId`.
  it('badges the plate the SERVER resolved as the portrait', async () => {
    const fixture = await render(
      stubClient({
        rolls: [roll({ fileId: 'a' }), roll({ fileId: 'b', isPortrait: true })],
      }).client,
    );
    await expand(fixture);
    expect(text(fixture)).toContain('Avatar');
    // The portrait tile offers no Set-as-avatar button (v4 `!isAvatar`).
    const setButtons = fixture.nativeElement.querySelectorAll('button[title="Set as avatar"]');
    expect(setButtons).toHaveLength(1);
  });

  // v4 GalleryImage:99-100 — the keep button's two labels.
  it('labels the keep button by album membership, and disables the kept one', async () => {
    const fixture = await render(
      stubClient({
        rolls: [roll({ fileId: 'a' }), roll({ fileId: 'b', albumLinkId: 'album-1' })],
      }).client,
    );
    await expand(fixture);
    const keep = buttonByTitle(fixture, 'Keep in the photo album')!;
    const kept = buttonByTitle(fixture, 'Already in the photo album')!;
    expect(keep.disabled).toBe(false);
    expect(kept.disabled).toBe(true);
    // v4 sets `aria-label` to the same sentence as `title`.
    expect(kept.getAttribute('aria-label')).toBe('Already in the photo album');
    // The kept tile reads done: the filled success pair, not the card pair.
    expect(kept.className).toContain('qt-bg-success');
    expect(keep.className).toContain('hover:qt-bg-primary');
  });

  // Tier 2 (order item 6): `usedInChatCount` reaches the tooltip and nowhere else.
  it('shows the in-use count in the delete tooltip and NOWHERE in the tile text', async () => {
    const fixture = await render(
      stubClient({ rolls: [roll({ usedInChatCount: 4 })] }).client,
    );
    await expand(fixture);
    expect(buttonByTitle(fixture, 'Discard this plate (in use in 4 conversations)')).not.toBeNull();
    // Not "4" anywhere in the rendered text — only "1 plate" is a number here.
    const grid = fixture.nativeElement.querySelector('.grid') as HTMLElement;
    expect(grid.textContent).not.toContain('4');
  });

  // -------------------------------------------------------------------------
  // `describeDelete` — v4 `:103-113`, all four shapes
  // -------------------------------------------------------------------------

  it('describes a plain plate with no parenthetical', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    await expand(fixture);
    expect(buttonByTitle(fixture, 'Discard this plate')).not.toBeNull();
  });

  it('describes an in-use plate in the singular', async () => {
    const fixture = await render(stubClient({ rolls: [roll({ usedInChatCount: 1 })] }).client);
    await expand(fixture);
    expect(
      buttonByTitle(fixture, 'Discard this plate (in use in 1 conversation)'),
    ).not.toBeNull();
  });

  it('describes a kept plate', async () => {
    const fixture = await render(stubClient({ rolls: [roll({ albumLinkId: 'a1' })] }).client);
    await expand(fixture);
    expect(buttonByTitle(fixture, 'Discard this plate (the album copy stays)')).not.toBeNull();
  });

  it('joins both clauses with a semicolon and a space', async () => {
    const fixture = await render(
      stubClient({ rolls: [roll({ usedInChatCount: 2, albumLinkId: 'a1' })] }).client,
    );
    await expand(fixture);
    expect(
      buttonByTitle(
        fixture,
        'Discard this plate (in use in 2 conversations; the album copy stays)',
      ),
    ).not.toBeNull();
  });

  // -------------------------------------------------------------------------
  // The four actions
  // -------------------------------------------------------------------------

  it('keeps a plate in the album and says so', async () => {
    const stub = stubClient({
      rolls: [roll()],
      reply: () => ({ linkId: 'album-1', alreadyInAlbum: false }),
    });
    const fixture = await render(stub.client);
    await expand(fixture);
    buttonByTitle(fixture, 'Keep in the photo album')!.click();
    await flush(fixture);
    expect(stub.seen[1]).toEqual({
      type: 'characterAvatarRollAction',
      characterId: 'c1',
      fileId: 'f1',
      action: 'save-to-album',
    });
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Kept in the photo album' });
  });

  // v4 `:130` — the idempotent arm has its own sentence.
  it('says "Already in the album" when the server reports it was already kept', async () => {
    const stub = stubClient({
      rolls: [roll()],
      reply: () => ({ linkId: 'album-1', alreadyInAlbum: true }),
    });
    const fixture = await render(stub.client);
    await expand(fixture);
    buttonByTitle(fixture, 'Keep in the photo album')!.click();
    await flush(fixture);
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Already in the album' });
  });

  it('promotes a plate to the portrait, emits the ALBUM link id, and says so', async () => {
    const stub = stubClient({
      rolls: [roll()],
      reply: () => ({ linkId: 'album-7', addedToAlbum: true }),
    });
    const fixture = await render(stub.client);
    const seenAvatars: (string | null)[] = [];
    fixture.componentInstance.avatarChange.subscribe((v) => seenAvatars.push(v));
    await expand(fixture);
    buttonByTitle(fixture, 'Set as avatar')!.click();
    await flush(fixture);
    expect(stub.seen[1]!['action']).toBe('set-avatar');
    expect(seenAvatars).toEqual(['album-7']);
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Avatar updated!' });
  });

  // v4 `:142` — `if (!result?.linkId) return`: a silent bail, no toast.
  it('bails SILENTLY when set-avatar answers without a linkId', async () => {
    const stub = stubClient({ rolls: [roll()], reply: () => ({ addedToAlbum: true }) });
    const fixture = await render(stub.client);
    const seenAvatars: (string | null)[] = [];
    fixture.componentInstance.avatarChange.subscribe((v) => seenAvatars.push(v));
    await expand(fixture);
    buttonByTitle(fixture, 'Set as avatar')!.click();
    await flush(fixture);
    expect(seenAvatars).toEqual([]);
    expect(toasts()).toEqual([]);
  });

  // v4 `:112` — the refusal toasts the server's own sentence.
  it('toasts the server’s error when an action is refused', async () => {
    const stub = stubClient({
      rolls: [roll()],
      reply: () => {
        throw new CoreDispatchError({ kind: 'notFound', message: 'Avatar roll not found' });
      },
    });
    const fixture = await render(stub.client);
    await expand(fixture);
    buttonByTitle(fixture, 'Keep in the photo album')!.click();
    await flush(fixture);
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Avatar roll not found' });
  });

  it('downloads the bytes and hands them to the shell', async () => {
    const blob = new Blob(['x']);
    const fetched: string[] = [];
    const fetchStub = vi.fn(async (input: unknown) => {
      fetched.push(String(input));
      return { ok: true, blob: async () => blob } as unknown as Response;
    });
    vi.stubGlobal('fetch', fetchStub);
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    await expand(fixture);
    buttonByTitle(fixture, 'Download image')!.click();
    await flush(fixture);
    expect(fetchStub).toHaveBeenCalledTimes(1);
    expect(fetched[0]).toContain('images%2Fhistory%2Fa.webp');
  });

  // v4 `:186` — the failure sentence is FIXED, never the thrown message.
  it('toasts a fixed sentence when the download fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: false, status: 503 }) as unknown as Response),
    );
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    await expand(fixture);
    buttonByTitle(fixture, 'Download image')!.click();
    await flush(fixture);
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Failed to download image' });
  });

  // -------------------------------------------------------------------------
  // The two-click delete
  // -------------------------------------------------------------------------

  it('ARMS on the first click and writes nothing', async () => {
    const stub = stubClient({ rolls: [roll()], reply: () => ({ deleted: true }) });
    const fixture = await render(stub.client);
    await expand(fixture);
    buttonByTitle(fixture, 'Discard this plate')!.click();
    await flush(fixture);
    expect(stub.seen).toHaveLength(1); // the list read only
    expect(buttonByTitle(fixture, 'Click again to confirm delete')).not.toBeNull();
  });

  it('discards on the SECOND click and says the plate went', async () => {
    const stub = stubClient({
      rolls: [roll()],
      reply: () => ({ deleted: true, blobRemoved: true, chatsScrubbed: 0, keptInAlbum: false }),
    });
    const fixture = await render(stub.client);
    await expand(fixture);
    buttonByTitle(fixture, 'Discard this plate')!.click();
    await flush(fixture);
    buttonByTitle(fixture, 'Click again to confirm delete')!.click();
    await flush(fixture);
    expect(stub.seen[1]).toEqual({
      type: 'characterAvatarRollDelete',
      characterId: 'c1',
      fileId: 'f1',
    });
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Roll discarded' });
  });

  // v4 `:166` — the kept-copy arm has its own sentence.
  it('says the album copy stays when the server reports it kept', async () => {
    const stub = stubClient({
      rolls: [roll({ albumLinkId: 'a1' })],
      reply: () => ({ deleted: true, keptInAlbum: true }),
    });
    const fixture = await render(stub.client);
    await expand(fixture);
    buttonByTitle(fixture, 'Discard this plate (the album copy stays)')!.click();
    await flush(fixture);
    buttonByTitle(fixture, 'Click again to confirm delete')!.click();
    await flush(fixture);
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'Roll discarded; the album copy stays',
    });
  });

  // v4 `:161` — `body?.error || 'Failed to delete the avatar roll'`.
  it('falls back to v4’s fixed sentence when a refusal carries no message', async () => {
    const stub = stubClient({
      rolls: [roll()],
      reply: () => {
        throw new CoreDispatchError({ kind: 'internal', message: '' });
      },
    });
    const fixture = await render(stub.client);
    await expand(fixture);
    buttonByTitle(fixture, 'Discard this plate')!.click();
    await flush(fixture);
    buttonByTitle(fixture, 'Click again to confirm delete')!.click();
    await flush(fixture);
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'Failed to delete the avatar roll',
    });
  });

  // -------------------------------------------------------------------------
  // The three invalidations — v4 `invalidateAll` (`:82-88`)
  // -------------------------------------------------------------------------

  it('invalidates avatarRolls, photos AND detail on every mutation', async () => {
    const queryClient = new QueryClient();
    const invalidated: string[] = [];
    vi.spyOn(queryClient, 'invalidateQueries').mockImplementation(((opts: {
      queryKey: unknown[];
    }) => {
      invalidated.push(JSON.stringify(opts.queryKey));
      return Promise.resolve();
    }) as never);
    const stub = stubClient({ rolls: [roll()], reply: () => ({ linkId: 'l1' }) });
    const fixture = await render(stub.client, { queryClient });
    await expand(fixture);
    buttonByTitle(fixture, 'Keep in the photo album')!.click();
    await flush(fixture);
    expect(invalidated).toContain(JSON.stringify(['characters', 'avatar-rolls', 'c1']));
    expect(invalidated).toContain(JSON.stringify(['characters', 'photos', 'c1']));
    expect(invalidated).toContain(JSON.stringify(['characters', 'detail', 'c1']));
  });

  it('invalidates all three on a delete too', async () => {
    const queryClient = new QueryClient();
    const invalidated: string[] = [];
    vi.spyOn(queryClient, 'invalidateQueries').mockImplementation(((opts: {
      queryKey: unknown[];
    }) => {
      invalidated.push(JSON.stringify(opts.queryKey));
      return Promise.resolve();
    }) as never);
    const stub = stubClient({ rolls: [roll()], reply: () => ({ deleted: true }) });
    const fixture = await render(stub.client, { queryClient });
    await expand(fixture);
    buttonByTitle(fixture, 'Discard this plate')!.click();
    await flush(fixture);
    buttonByTitle(fixture, 'Click again to confirm delete')!.click();
    await flush(fixture);
    expect(invalidated).toContain(JSON.stringify(['characters', 'detail', 'c1']));
  });

  // -------------------------------------------------------------------------
  // The refresh output — v4's `onRefresh?.()` after every mutation
  // -------------------------------------------------------------------------

  it('emits refresh after a keep, a set-avatar and a delete, but not after a download', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: true, blob: async () => new Blob(['x']) }) as unknown as Response),
    );
    const stub = stubClient({ rolls: [roll()], reply: () => ({ linkId: 'l1', deleted: true }) });
    const fixture = await render(stub.client);
    let refreshes = 0;
    fixture.componentInstance.refresh.subscribe(() => (refreshes += 1));
    await expand(fixture);

    buttonByTitle(fixture, 'Download image')!.click();
    await flush(fixture);
    expect(refreshes).toBe(0);

    buttonByTitle(fixture, 'Keep in the photo album')!.click();
    await flush(fixture);
    expect(refreshes).toBe(1);

    buttonByTitle(fixture, 'Set as avatar')!.click();
    await flush(fixture);
    expect(refreshes).toBe(2);

    buttonByTitle(fixture, 'Discard this plate')!.click();
    await flush(fixture);
    expect(refreshes).toBe(2); // arming is not a mutation
    buttonByTitle(fixture, 'Click again to confirm delete')!.click();
    await flush(fixture);
    expect(refreshes).toBe(3);
  });

  // -------------------------------------------------------------------------
  // The detail modal hand-off — v4 `:171-202`
  // -------------------------------------------------------------------------

  it('opens the SAME detail modal the album uses, with no prompt field', async () => {
    const fixture = await render(
      stubClient({ rolls: [roll({ generationPrompt: 'ZZTOPSECRETPROMPT' })] }).client,
    );
    await expand(fixture);
    (fixture.nativeElement.querySelector('.grid button') as HTMLButtonElement).click();
    await flush(fixture);
    // The modal PORTALS to document.body, so it is NOT in this component's
    // subtree — querying the fixture would silently pass forever (the gallery
    // tab's spec records the same trap).
    const modal = document.querySelector('qt-image-detail-modal');
    expect(modal).toBeTruthy();
    expect(modal!.parentElement).toBe(document.body);
    // §R.6(8): v4 ships no generation-prompt viewer — the prompt the wire
    // carries reaches no rendered surface, the modal's included.
    expect(document.body.textContent).not.toContain('ZZTOPSECRETPROMPT');
  });

  // v4 `:176-178` — "A roll is an images-v2 row, so no `linkId`".
  it('hands the modal a FILE id and no linkId', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    await expand(fixture);
    (fixture.nativeElement.querySelector('.grid button') as HTMLButtonElement).click();
    await flush(fixture);
    const modal = fixture.debugElement.query(
      (n) => n.name === 'qt-image-detail-modal',
    );
    const image = modal.componentInstance.image() as Record<string, unknown>;
    expect(image['id']).toBe('f1');
    expect(image['linkId']).toBeUndefined();
    expect(image['tags']).toEqual([]);
  });

  // -------------------------------------------------------------------------
  // The load-error path — v4 `handleRollError` (`:90-93`)
  // -------------------------------------------------------------------------

  it('marks a roll missing when its bytes fail to load, and warns', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    await expand(fixture);
    const img = fixture.nativeElement.querySelector('img') as HTMLImageElement;
    img.dispatchEvent(new Event('error'));
    await flush(fixture);
    expect(fixture.nativeElement.querySelector('img')).toBeNull();
    expect(warn).toHaveBeenCalledWith('Avatar roll failed to load', { rollId: 'f1' });
    // A missing tile loses its Download button (v4 `!isMissingImage`).
    expect(buttonByTitle(fixture, 'Download image')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// The 3 s disarm — its own block, on fake timers
// ---------------------------------------------------------------------------

describe('AvatarRollsSection — the armed delete disarms after 3 s (v4 :55-60)', () => {
  afterEach(() => {
    vi.useRealTimers();
    TestBed.resetTestingModule();
  });

  /**
   * Fake timers are installed AFTER the render, not in a `beforeEach`: the
   * render and the expand both await real macrotasks to settle the query, and
   * a `shouldAdvanceTime` clock advances through those too — which pushed the
   * "still armed at 2 999 ms" boundary past 3 000 ms on the first run. With
   * the clock installed after, arming and disarming are pure synchronous
   * signal writes and the boundary is exact.
   */
  it('is still armed at 2 999 ms and disarmed at 3 000 ms', async () => {
    const fixture = await render(stubClient({ rolls: [roll()] }).client);
    await expand(fixture);

    vi.useFakeTimers();
    buttonByTitle(fixture, 'Discard this plate')!.click();
    fixture.detectChanges();
    expect(buttonByTitle(fixture, 'Click again to confirm delete')).not.toBeNull();

    vi.advanceTimersByTime(2999);
    fixture.detectChanges();
    expect(buttonByTitle(fixture, 'Click again to confirm delete')).not.toBeNull();

    vi.advanceTimersByTime(1);
    fixture.detectChanges();
    expect(buttonByTitle(fixture, 'Click again to confirm delete')).toBeNull();
    expect(buttonByTitle(fixture, 'Discard this plate')).not.toBeNull();
  });
});
