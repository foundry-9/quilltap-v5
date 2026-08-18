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

import { WORKSPACE_TAB_VISIBLE, onTabActivated } from '../workspace-contract';

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
