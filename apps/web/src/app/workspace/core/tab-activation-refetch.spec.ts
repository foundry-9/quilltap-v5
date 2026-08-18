/**
 * Tab re-activation refresh: navigating back to a kept-alive tab must refresh
 * its data sources. The 1:1 mirror of v4's
 * `__tests__/unit/components/workspace/tab-activation-refetch.test.tsx`
 * (baseline `979652a9`), adapted to v5's keys. Two layers under test:
 *
 *  1. {@link onTabActivated} — fires on every hidden→visible transition of the
 *     containing tab, never on the first observed visibility, and never
 *     outside the workspace.
 *  2. `tabActivationQueryKeys` — the kind→query-key-prefix map that the tab
 *     host's invalidator feeds to TanStack Query, including the deliberate
 *     blanks (live streams, editors with unsaved state).
 *
 * v4's React probe re-renders with a NEW callback to prove the hook never runs
 * a stale one; v5 registers once in an injection context, so the equivalent
 * guarantee is that the callback reads its state AT FIRE TIME (test 3).
 */

import { Component, signal, type WritableSignal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { WORKSPACE_TAB_VISIBLE, onTabActivated, type TabKind, type WorkspaceTab } from '../workspace-contract';
import { characterKeys, tagKeys } from '../../screens/characters/characters.api';
import { groupKeys } from '../../screens/groups/groups.api';
import { homeKeys } from '../../screens/home/home.api';
import { imageProfileKeys } from '../../screens/settings/images/image-profiles.api';
import { profileKeys } from '../../screens/profile/profile.api';
import { projectKeys } from '../../screens/prospero/projects.api';
import { templateKeys } from '../../screens/settings/templates/templates.api';
import { DEFAULT_TAB_META } from './tab-meta';
import { tabActivationQueryKeys } from './tab-refetch';

/** What the probe's callback does — set before each `createComponent`. */
let activation: () => void = () => {};

@Component({ selector: 'qt-activation-probe', template: '' })
class Probe {
  constructor() {
    onTabActivated(() => activation());
  }
}

/** Mount the probe under a visibility signal (omit ⇒ outside the workspace). */
function mountProbe(visible?: WritableSignal<boolean>) {
  TestBed.configureTestingModule({
    providers: visible ? [{ provide: WORKSPACE_TAB_VISIBLE, useValue: visible }] : [],
  });
  const fixture = TestBed.createComponent(Probe);
  fixture.detectChanges();
  return fixture;
}

describe('onTabActivated', () => {
  it('does not fire on the initial mount', () => {
    const fired: number[] = [];
    activation = () => fired.push(1);
    mountProbe(signal(true));
    expect(fired).toEqual([]);
  });

  it('fires on every hidden→visible transition, not on visible→hidden', () => {
    const fired: number[] = [];
    activation = () => fired.push(1);
    const visible = signal(true);
    const fixture = mountProbe(visible);

    visible.set(false);
    fixture.detectChanges();
    expect(fired.length).toBe(0);

    visible.set(true);
    fixture.detectChanges();
    expect(fired.length).toBe(1);

    visible.set(false);
    fixture.detectChanges();
    visible.set(true);
    fixture.detectChanges();
    expect(fired.length).toBe(2);
  });

  it('invokes the callback against the state it has at activation, not at registration', () => {
    const seen: string[] = [];
    const label = signal('first');
    activation = () => seen.push(label());
    const visible = signal(true);
    const fixture = mountProbe(visible);

    visible.set(false);
    fixture.detectChanges();
    label.set('second');
    fixture.detectChanges();
    visible.set(true);
    fixture.detectChanges();

    expect(seen).toEqual(['second']);
  });

  it('never fires outside the workspace (no visibility token)', () => {
    const fired: number[] = [];
    activation = () => fired.push(1);
    const fixture = mountProbe();
    fixture.detectChanges();
    fixture.detectChanges();
    expect(fired).toEqual([]);
  });
});

describe('tabActivationQueryKeys', () => {
  const tab = (kind: TabKind, payload?: unknown): WorkspaceTab =>
    ({ id: 't1', kind, payload, title: 'x' }) as WorkspaceTab;

  /**
   * Every kind the workspace can host, read from the `Record<TabKind, …>` the
   * tab-meta table is typed as — so a kind added to the union cannot escape
   * these assertions, and one silently dropped from the map reds them (the
   * `harness-corpus-shape-constants-rot` rule: assert over the real union, not
   * a transcribed sample).
   */
  const ALL_KINDS = Object.keys(DEFAULT_TAB_META) as TabKind[];

  /** The kinds this map deliberately refreshes (everything else must be `[]`). */
  const REFRESHING_KINDS: TabKind[] = [
    'home',
    'salon-list',
    'aurora',
    'character-view',
    'prospero',
    'files',
    'generate-image',
    'custom-tools',
    'profile',
    'settings',
  ];

  /** `character-view` is the one kind whose entry is payload-scoped. */
  const payloadFor = (kind: TabKind): unknown =>
    kind === 'character-view' ? { characterId: 'c9' } : undefined;

  it('refreshes the home dashboard and the entities it summarizes', () => {
    expect(tabActivationQueryKeys(tab('home'))).toEqual([
      homeKeys.all,
      ['chats'],
      projectKeys.all,
      characterKeys.all,
    ]);
  });

  it('uses the chats collection prefix, never a per-chat key that would sweep detail/state around a live stream', () => {
    for (const kind of ['home', 'salon-list'] as const) {
      const keys = tabActivationQueryKeys(tab(kind));
      expect(keys).toContainEqual(['chats']);
      // v5's per-chat keys are `['chat', id]` SINGULAR, so no prefix here can
      // reach them (v4 narrows to `['chats','list']` for the same reason).
      expect(keys.some((k) => k[0] === 'chat')).toBe(false);
    }
  });

  it('scopes character-view invalidation to the payload character', () => {
    expect(tabActivationQueryKeys(tab('character-view', { characterId: 'c9' }))).toEqual([
      characterKeys.detail('c9'),
      characterKeys.prompts('c9'),
      characterKeys.photos('c9'),
    ]);
    expect(tabActivationQueryKeys(tab('character-view'))).toEqual([]);
  });

  it('refreshes exactly the declared roster — every other kind in the union is empty', () => {
    const refreshing = ALL_KINDS.filter(
      (k) => tabActivationQueryKeys(tab(k, payloadFor(k))).length > 0,
    );
    expect([...refreshing].sort()).toEqual([...REFRESHING_KINDS].sort());

    // The complement, by name: v4's deliberately-empty roster verbatim, plus
    // v5's three additions — `scriptorium` / `photos` (hand-rolled in v5, they
    // re-run their own loads via onTabActivated) and `salon-new` (a form
    // holding unsaved input, an editor by v4's own reasoning).
    const empty = ALL_KINDS.filter((k) => !REFRESHING_KINDS.includes(k));
    expect([...empty].sort()).toEqual(
      [
        'about',
        'brahma',
        'character-edit',
        'character-new',
        'document',
        'document-standalone',
        'photos',
        'salon',
        'salon-new',
        'scenarios',
        'scriptorium',
        'settings-wizard',
        'terminal',
        'wardrobe',
      ].sort(),
    );
    for (const kind of empty) {
      expect(tabActivationQueryKeys(tab(kind))).toEqual([]);
    }
  });

  it('invalidates BOTH live spellings of every split key (v5-only)', () => {
    const settings = tabActivationQueryKeys(tab('settings'));
    for (const pair of [
      [['connectionProfiles'], ['connection-profiles']],
      [['apiKeys'], ['api-keys']],
      [['chatSettings'], ['chat-settings']],
    ]) {
      for (const spelling of pair) {
        expect(settings).toContainEqual(spelling);
      }
    }
    // The salon list reads chat settings and connection profiles too.
    const salonList = tabActivationQueryKeys(tab('salon-list'));
    for (const spelling of [
      ['connectionProfiles'],
      ['connection-profiles'],
      ['chatSettings'],
      ['chat-settings'],
    ]) {
      expect(salonList).toContainEqual(spelling);
    }
  });

  it('covers the settings tab cards that have a v5 query key', () => {
    expect(tabActivationQueryKeys(tab('settings'))).toEqual([
      ['chatSettings'],
      ['chat-settings'],
      ['textReplacements'],
      ['connectionProfiles'],
      ['connection-profiles'],
      ['embedding-profiles'],
      imageProfileKeys.all,
      ['providers'],
      ['apiKeys'],
      ['api-keys'],
      templateKeys.all,
    ]);
  });

  it('gives the singleton list tabs their entity prefix', () => {
    expect(tabActivationQueryKeys(tab('aurora'))).toEqual([
      characterKeys.all,
      groupKeys.all,
      tagKeys.all,
    ]);
    expect(tabActivationQueryKeys(tab('prospero'))).toEqual([projectKeys.all]);
    expect(tabActivationQueryKeys(tab('files'))).toEqual([['files']]);
    expect(tabActivationQueryKeys(tab('generate-image'))).toEqual([
      characterKeys.all,
      imageProfileKeys.all,
    ]);
    expect(tabActivationQueryKeys(tab('custom-tools'))).toEqual([['customTools']]);
    expect(tabActivationQueryKeys(tab('profile'))).toEqual([profileKeys.detail]);
  });
});
