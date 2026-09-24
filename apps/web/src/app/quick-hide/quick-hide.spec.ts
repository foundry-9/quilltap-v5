import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { TagDto } from '../core/core-contract';
import { QuickHideService } from './quick-hide.service';
import {
  ACTIVE_TAGS_KEY,
  HIDE_DANGEROUS_KEY,
  HIDE_SALON_IMAGES_KEY,
  INCLUDE_AUTONOMOUS_KEY,
  parseActiveTags,
} from './quick-hide.storage';
import { shouldHideByIds } from './should-hide';

// ---------------------------------------------------------------------------
// The pure predicate (v4 `quick-hide-provider.tsx:183-196`)
// ---------------------------------------------------------------------------

describe('shouldHideByIds (v4 quick-hide-provider.tsx:183-196)', () => {
  const hidden = new Set(['t-hidden']);

  it('is false when there are no ids at all (v4 :185-187)', () => {
    expect(shouldHideByIds(hidden, undefined)).toBe(false);
    expect(shouldHideByIds(hidden, [])).toBe(false);
  });

  it('is true when ANY id is hidden (v4 :188-192)', () => {
    expect(shouldHideByIds(hidden, ['t-visible', 't-hidden'])).toBe(true);
    expect(shouldHideByIds(hidden, ['t-hidden'])).toBe(true);
  });

  it('is false when no id is hidden', () => {
    expect(shouldHideByIds(hidden, ['t-a', 't-b'])).toBe(false);
  });

  it('skips nullish entries rather than matching on them (v4 :189 `if (tagId && …)`)', () => {
    expect(shouldHideByIds(hidden, [null, undefined, ''])).toBe(false);
    expect(shouldHideByIds(hidden, [null, 't-hidden'])).toBe(true);
  });

  it('an empty hidden set hides nothing', () => {
    expect(shouldHideByIds(new Set(), ['t-hidden'])).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The storage substrate (v4 `:92-129`)
// ---------------------------------------------------------------------------

describe('parseActiveTags (v4 quick-hide-provider.tsx:96-101)', () => {
  it('keeps only string entries (v4 :100)', () => {
    expect([...parseActiveTags('["a", 1, null, "b", {}]')]).toEqual(['a', 'b']);
  });

  it('falls back to empty on absent / malformed / non-array JSON (v4 :98,:111)', () => {
    expect(parseActiveTags(null).size).toBe(0);
    expect(parseActiveTags('not json').size).toBe(0);
    expect(parseActiveTags('{"a":1}').size).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// The service (v4 `QuickHideProvider`)
// ---------------------------------------------------------------------------

/** A minimal in-memory localStorage stub, installed before the service reads it. */
function stubStorage(seed: Record<string, string> = {}): Map<string, string> {
  const store = new Map<string, string>(Object.entries(seed));
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
  return store;
}

function tag(over: Partial<TagDto>): TagDto {
  return { id: 't1', name: 'Tag', quickHide: true, ...over };
}

/** Stand the service up over a canned `tagList` response. */
async function makeService(tags: TagDto[] = []): Promise<QuickHideService> {
  const client: Partial<CoreClient> = {
    dispatchData: (async () => ({ tags })) as CoreClient['dispatchData'],
  };
  TestBed.configureTestingModule({
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const service = TestBed.inject(QuickHideService);
  // The constructor's `refresh()` is fire-and-forget; let it settle.
  await service.refresh();
  return service;
}

describe('QuickHideService', () => {
  beforeEach(() => TestBed.resetTestingModule());
  afterEach(() => vi.unstubAllGlobals());

  describe('the tag source (v4 :37-47)', () => {
    it('keeps only tags flagged quickHide (v4 :45 `Boolean(tag.quickHide)`)', async () => {
      stubStorage();
      const service = await makeService([
        tag({ id: 'a', name: 'Alpha', quickHide: true }),
        tag({ id: 'b', name: 'Beta', quickHide: false }),
        tag({ id: 'c', name: 'Gamma', quickHide: undefined }),
      ]);
      expect(service.quickHideTags()).toEqual([{ id: 'a', name: 'Alpha' }]);
    });

    it('fails soft to an empty list when the load throws (v4 :76-78)', async () => {
      stubStorage();
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const client: Partial<CoreClient> = {
        dispatchData: (async () => {
          throw new Error('nope');
        }) as CoreClient['dispatchData'],
      };
      TestBed.configureTestingModule({
        providers: [{ provide: CoreClient, useValue: client }],
      });
      const service = TestBed.inject(QuickHideService);
      await service.refresh();
      expect(service.quickHideTags()).toEqual([]);
      expect(service.loading()).toBe(false);
      expect(warn).toHaveBeenCalled();
      warn.mockRestore();
    });

    it('prunes hidden ids that are no longer valid quickHide tags (v4 :71-75)', async () => {
      // 'gone' was hidden in a previous session but is no longer flagged.
      stubStorage({ [ACTIVE_TAGS_KEY]: JSON.stringify(['a', 'gone']) });
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      expect([...service.hiddenTagIds()]).toEqual(['a']);
    });

    it('preserves set identity when the prune changes nothing (v4 :74)', async () => {
      stubStorage({ [ACTIVE_TAGS_KEY]: JSON.stringify(['a']) });
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      const before = service.hiddenTagIds();
      await service.refresh();
      expect(service.hiddenTagIds()).toBe(before);
    });
  });

  describe('eager localStorage hydration (v4 :92-116, read eagerly in v5)', () => {
    it('hydrates all three keys from storage', async () => {
      stubStorage({
        [ACTIVE_TAGS_KEY]: JSON.stringify(['a']),
        [HIDE_DANGEROUS_KEY]: 'true',
        [INCLUDE_AUTONOMOUS_KEY]: 'true',
      });
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      expect([...service.hiddenTagIds()]).toEqual(['a']);
      expect(service.hideDangerousChats()).toBe(true);
      expect(service.includeAutonomousRooms()).toBe(true);
    });

    it("treats only the literal 'true' as truthy (v4 :104,:108)", async () => {
      stubStorage({ [HIDE_DANGEROUS_KEY]: '1', [INCLUDE_AUTONOMOUS_KEY]: 'yes' });
      const service = await makeService();
      expect(service.hideDangerousChats()).toBe(false);
      expect(service.includeAutonomousRooms()).toBe(false);
    });

    it('defaults everything off with empty storage', async () => {
      stubStorage();
      const service = await makeService();
      expect(service.hiddenTagIds().size).toBe(0);
      expect(service.hideDangerousChats()).toBe(false);
      expect(service.includeAutonomousRooms()).toBe(false);
    });
  });

  describe('the toggles (v4 :155-174)', () => {
    it('toggleTag adds then removes, persisting the id array (v4 :155-166,:123)', async () => {
      const store = stubStorage();
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);

      service.toggleTag('a');
      expect(service.shouldHideByIds(['a'])).toBe(true);
      expect(store.get(ACTIVE_TAGS_KEY)).toBe(JSON.stringify(['a']));

      service.toggleTag('a');
      expect(service.shouldHideByIds(['a'])).toBe(false);
      expect(store.get(ACTIVE_TAGS_KEY)).toBe(JSON.stringify([]));
    });

    it("toggleHideDangerousChats flips and persists 'true'/'false' (v4 :168-170,:124)", async () => {
      const store = stubStorage();
      const service = await makeService();
      service.toggleHideDangerousChats();
      expect(service.hideDangerousChats()).toBe(true);
      expect(store.get(HIDE_DANGEROUS_KEY)).toBe('true');
      service.toggleHideDangerousChats();
      expect(store.get(HIDE_DANGEROUS_KEY)).toBe('false');
    });

    it('toggleIncludeAutonomousRooms flips and persists the shared key (v4 :172-174,:125)', async () => {
      const store = stubStorage();
      const service = await makeService();
      service.toggleIncludeAutonomousRooms();
      expect(service.includeAutonomousRooms()).toBe(true);
      expect(store.get(INCLUDE_AUTONOMOUS_KEY)).toBe('true');
    });
  });

  describe('clearAllHidden (v4 :176-181)', () => {
    it('clears the tags and the danger filter but SPARES includeAutonomousRooms (v4 :179-180)', async () => {
      const store = stubStorage();
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      service.toggleTag('a');
      service.toggleHideDangerousChats();
      service.toggleIncludeAutonomousRooms();

      service.clearAllHidden();

      expect(service.hiddenTagIds().size).toBe(0);
      expect(service.hideDangerousChats()).toBe(false);
      // The "include" toggle ADDS items, so Clear All Hidden must not reset it.
      expect(service.includeAutonomousRooms()).toBe(true);
      expect(store.get(INCLUDE_AUTONOMOUS_KEY)).toBe('true');
      expect(store.get(ACTIVE_TAGS_KEY)).toBe(JSON.stringify([]));
      expect(store.get(HIDE_DANGEROUS_KEY)).toBe('false');
    });
  });

  /**
   * v4 `e3937d7aa`'s `salon-images.test.tsx` — the four `QuickHideProvider`
   * vectors, transcribed against v4's storage key and value bytes. v4 loads
   * after mount; v5 reads eagerly (the file-level divergence above), so
   * "restores a stored choice on load" seeds storage before construction.
   */
  describe('hideSalonImages (v4 e3937d7aa salon-images.test.tsx)', () => {
    it('is the v4 storage key, byte for byte', () => {
      expect(HIDE_SALON_IMAGES_KEY).toBe('quilltap.quickHide.hideSalonImages');
    });

    it('shows images by default', async () => {
      stubStorage();
      const service = await makeService();
      expect(service.hideSalonImages()).toBe(false);
    });

    it('toggles and persists the choice to localStorage', async () => {
      const store = stubStorage();
      const service = await makeService();
      service.toggleHideSalonImages();
      expect(service.hideSalonImages()).toBe(true);
      expect(store.get(HIDE_SALON_IMAGES_KEY)).toBe('true');

      service.toggleHideSalonImages();
      expect(service.hideSalonImages()).toBe(false);
      expect(store.get(HIDE_SALON_IMAGES_KEY)).toBe('false');
    });

    it('restores a stored choice on load', async () => {
      stubStorage({ [HIDE_SALON_IMAGES_KEY]: 'true' });
      const service = await makeService();
      expect(service.hideSalonImages()).toBe(true);
    });

    it("treats only the literal 'true' as truthy (v4 :127 `=== 'true'`)", async () => {
      stubStorage({ [HIDE_SALON_IMAGES_KEY]: '1' });
      const service = await makeService();
      expect(service.hideSalonImages()).toBe(false);
    });

    it('is reset by Clear All Hidden (v4 :206)', async () => {
      const store = stubStorage({ [HIDE_SALON_IMAGES_KEY]: 'true' });
      const service = await makeService();
      expect(service.hideSalonImages()).toBe(true);
      service.clearAllHidden();
      expect(service.hideSalonImages()).toBe(false);
      expect(store.get(HIDE_SALON_IMAGES_KEY)).toBe('false');
    });

    it('adopts another tab’s write, and ignores a cleared key (v4 :170-172)', async () => {
      stubStorage();
      const service = await makeService();
      window.dispatchEvent(
        new StorageEvent('storage', { key: HIDE_SALON_IMAGES_KEY, newValue: 'true' }),
      );
      expect(service.hideSalonImages()).toBe(true);
      window.dispatchEvent(
        new StorageEvent('storage', { key: HIDE_SALON_IMAGES_KEY, newValue: null }),
      );
      expect(service.hideSalonImages()).toBe(true);
      window.dispatchEvent(
        new StorageEvent('storage', { key: HIDE_SALON_IMAGES_KEY, newValue: 'false' }),
      );
      expect(service.hideSalonImages()).toBe(false);
    });
  });

  describe('the badge state (v4 sidebar-footer.tsx:144)', () => {
    it('hasAnyHidden covers hidden tags, the danger filter OR Salon Images (v4 e3937d7aa :144)', async () => {
      stubStorage();
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      expect(service.hasAnyHidden()).toBe(false);
      service.toggleTag('a');
      expect(service.hasAnyHidden()).toBe(true);
      service.clearAllHidden();
      expect(service.hasAnyHidden()).toBe(false);
      service.toggleHideDangerousChats();
      expect(service.hasAnyHidden()).toBe(true);
      service.clearAllHidden();
      expect(service.hasAnyHidden()).toBe(false);
      service.toggleHideSalonImages();
      expect(service.hasAnyHidden()).toBe(true);
    });

    it('never dispatches the retired chatsHasDangerous probe (v4 e3937d7aa / 944127d9a)', async () => {
      stubStorage();
      const seen: string[] = [];
      const client: Partial<CoreClient> = {
        dispatchData: (async (req: { type: string }) => {
          seen.push(req.type);
          return { tags: [] };
        }) as unknown as CoreClient['dispatchData'],
      };
      TestBed.configureTestingModule({
        providers: [{ provide: CoreClient, useValue: client }],
      });
      const service = TestBed.inject(QuickHideService);
      await service.refresh();
      // The constructor's load + the explicit one — tags only.
      expect(seen).toEqual(['tagList', 'tagList']);
    });
  });

  /**
   * The tag load's fail-soft warn is v4's own line. P4.69 also pinned the
   * silence of the `has-dangerous` probe's catch; that case was RETIRED at
   * P4.D223 because the probe itself is gone (v4 deleted `useHasDangerousChats`
   * in `e3937d7aa`), so there is no second fail-soft path left to pin.
   */
  describe('the tag-load warn (v4 :82)', () => {
    it('still warns when the TAG load fails — that one is v4’s own line (v4 :82)', async () => {
      stubStorage();
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const client: Partial<CoreClient> = {
        dispatchData: (async () => {
          throw new Error('nope');
        }) as CoreClient['dispatchData'],
      };
      TestBed.configureTestingModule({
        providers: [{ provide: CoreClient, useValue: client }],
      });
      const service = TestBed.inject(QuickHideService);
      // The constructor fires a tag load, fire-and-forget; let it settle and
      // start the count from zero so the assertion below measures exactly one
      // `refresh()`.
      await service.refresh();
      warn.mockClear();

      await service.refresh();
      // Exactly one warn for one failed tag load, with v4's content.
      expect(warn).toHaveBeenCalledTimes(1);
      expect(warn.mock.calls[0]?.[0]).toBe('Unable to load quick-hide tags');
      expect(warn.mock.calls[0]?.[1]).toEqual({ error: 'nope' });
      warn.mockRestore();
    });
  });

  describe('the cross-tab storage listener (v4 :131-153)', () => {
    it('adopts another tab’s hidden-tag write (v4 :134-143)', async () => {
      stubStorage();
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      window.dispatchEvent(
        new StorageEvent('storage', { key: ACTIVE_TAGS_KEY, newValue: JSON.stringify(['a']) }),
      );
      expect([...service.hiddenTagIds()]).toEqual(['a']);
    });

    it('adopts the two boolean keys (v4 :144-149)', async () => {
      stubStorage();
      const service = await makeService();
      window.dispatchEvent(
        new StorageEvent('storage', { key: HIDE_DANGEROUS_KEY, newValue: 'true' }),
      );
      window.dispatchEvent(
        new StorageEvent('storage', { key: INCLUDE_AUTONOMOUS_KEY, newValue: 'true' }),
      );
      expect(service.hideDangerousChats()).toBe(true);
      expect(service.includeAutonomousRooms()).toBe(true);
    });

    it('IGNORES a cleared key — v4 guards each arm with `&& event.newValue`', async () => {
      stubStorage();
      const service = await makeService();
      service.toggleHideDangerousChats();
      window.dispatchEvent(
        new StorageEvent('storage', { key: HIDE_DANGEROUS_KEY, newValue: null }),
      );
      // Faithful to v4: a null newValue is not treated as "reset to false".
      expect(service.hideDangerousChats()).toBe(true);
    });

    it('ignores unrelated keys', async () => {
      stubStorage();
      const service = await makeService();
      window.dispatchEvent(
        new StorageEvent('storage', { key: 'something.else', newValue: 'true' }),
      );
      expect(service.hideDangerousChats()).toBe(false);
    });
  });
});
