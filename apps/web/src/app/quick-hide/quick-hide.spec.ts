import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { TagDto } from '../core/core-contract';
import { QuickHideService } from './quick-hide.service';
import {
  ACTIVE_TAGS_KEY,
  HIDE_DANGEROUS_KEY,
  INCLUDE_AUTONOMOUS_KEY,
  parseActiveTags,
} from './quick-hide.storage';
import { quickHideFeaturesVisible, shouldHideByIds } from './should-hide';

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

  describe('the badge state (v4 sidebar-footer.tsx:144-145)', () => {
    it('hasAnyHidden covers hidden tags OR the danger filter (v4 :145)', async () => {
      stubStorage();
      const service = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      expect(service.hasAnyHidden()).toBe(false);
      service.toggleTag('a');
      expect(service.hasAnyHidden()).toBe(true);
      service.clearAllHidden();
      expect(service.hasAnyHidden()).toBe(false);
      service.toggleHideDangerousChats();
      expect(service.hasAnyHidden()).toBe(true);
    });

    it('hasQuickHideFeatures is the three-way OR, third arm included (v4 :144, §H)', () => {
      // The service can only reach two of the three today — the uncensored-row
      // probe is gated ACTIVATE-AT-UNIFY on P4.D143's verb — so the rule is
      // pinned where it lives rather than through a wire that cannot move.
      expect(quickHideFeaturesVisible(false, false, false)).toBe(false);
      expect(quickHideFeaturesVisible(true, false, false)).toBe(true);
      expect(quickHideFeaturesVisible(false, true, false)).toBe(true);
      expect(quickHideFeaturesVisible(false, false, true)).toBe(true);
    });

    it('hasQuickHideFeatures covers a flagged tag existing OR the danger filter (v4 :144)', async () => {
      stubStorage();
      const withTag = await makeService([tag({ id: 'a', name: 'Alpha' })]);
      expect(withTag.hasQuickHideFeatures()).toBe(true);

      TestBed.resetTestingModule();
      const bare = await makeService([]);
      expect(bare.hasQuickHideFeatures()).toBe(false);
      bare.toggleHideDangerousChats();
      expect(bare.hasQuickHideFeatures()).toBe(true);
    });
  });

  /**
   * P4.69 — the two fail-soft paths log DIFFERENTLY, in v4 as here.
   *
   * v4's tag load warns (`quick-hide-provider.tsx:82` —
   * `console.warn('Unable to load quick-hide tags', { error: … })`, the exact
   * line v5 carries), but v4's `has-dangerous` probe is a bare `catch {}` whose
   * whole body is the comment "Silently ignore — worst case the quick-hide
   * button doesn't appear" (`use-has-dangerous-chats.ts:27-29`). v5 had invented
   * a `console.warn` on the probe path; P4.69 retired it and pins both halves
   * here, so neither can drift back.
   */
  describe('the fail-soft logging asymmetry (P4.69)', () => {
    it('says NOTHING when the has-dangerous probe fails (v4 use-has-dangerous-chats.ts:27-29)', async () => {
      stubStorage();
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      // Every dispatch throws, so BOTH the constructor's probe and its tag load
      // take their catch arms; `refresh()` is then driven explicitly below so
      // the tags warn is accounted for separately.
      const client: Partial<CoreClient> = {
        dispatchData: (async (req: { type: string }) => {
          if (req.type === 'chatsHasDangerous') throw new Error('probe down');
          return { tags: [] };
        }) as unknown as CoreClient['dispatchData'],
      };
      TestBed.configureTestingModule({
        providers: [{ provide: CoreClient, useValue: client }],
      });
      const service = TestBed.inject(QuickHideService);
      await service.refresh();
      // The probe failed and the answer stayed false...
      expect(service.hasDangerousChats()).toBe(false);
      // ...and not a word about it. The tag load succeeded here, so ANY warn on
      // this run is the retired invention.
      expect(warn).not.toHaveBeenCalled();
      warn.mockRestore();
    });

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
      // The constructor fires BOTH the probe and a tag load, fire-and-forget;
      // let them settle and start the count from zero so the assertion below
      // measures exactly one `refresh()`.
      await service.refresh();
      warn.mockClear();

      await service.refresh();
      // Exactly one warn for one failed tag load — the tags one. If the probe's
      // invented warn came back it would not appear here (the probe only runs
      // in the constructor), which is why the count is pinned in the sibling
      // case above and the CONTENT is pinned here.
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
