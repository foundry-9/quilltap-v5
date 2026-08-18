/**
 * TabView keep-alive — the mandatory mount-counter spec (v4's `TabView.tsx`
 * keep-alive suite). Proves the two invariants that let a streaming Salon
 * survive tab switches:
 *
 *  1. **Lazy-mount**: the hosted view is not constructed until the tab is first
 *     activated.
 *  2. **Keep-alive**: once mounted it is NEVER re-constructed — not when the tab
 *     is switched away (active → false) and back, and not when the reducer hands
 *     TabView a fresh state object with the SAME tab id (a payload refresh).
 *
 * Needs the Angular test env (TestBed); runs under `ng test` (jsdom).
 */

import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { TabView } from './tab-view';
import { TAB_VIEW_REGISTRY, type TabRegistry } from './tab-registry';
import type { TabKind, WorkspaceTab } from '../workspace-contract';

let constructions = 0;

@Component({ selector: 'qt-counting-stub', template: '<span data-stub></span>' })
class CountingStub {
  constructor() {
    constructions += 1;
  }
}

function registry(): TabRegistry {
  return { home: { component: CountingStub } } as unknown as TabRegistry;
}

function homeTab(): WorkspaceTab {
  return { id: 'home', kind: 'home' as TabKind, title: 'Home', icon: 'sparkles' };
}

describe('TabView keep-alive', () => {
  it('lazy-mounts, then never re-instantiates across active toggles and payload refreshes', () => {
    constructions = 0;
    TestBed.configureTestingModule({
      providers: [{ provide: TAB_VIEW_REGISTRY, useValue: registry() }],
    });
    const fixture = TestBed.createComponent(TabView);
    fixture.componentRef.setInput('tab', homeTab());
    fixture.componentRef.setInput('active', false);
    fixture.componentRef.setInput('visible', false);
    fixture.detectChanges();
    // (1) Lazy-mount: inactive ⇒ not constructed.
    expect(constructions).toBe(0);
    expect(fixture.nativeElement.querySelector('[data-stub]')).toBeNull();

    // First activation constructs it once.
    fixture.componentRef.setInput('active', true);
    fixture.componentRef.setInput('visible', true);
    fixture.detectChanges();
    expect(constructions).toBe(1);
    const stubEl = fixture.nativeElement.querySelector('[data-stub]');
    expect(stubEl).not.toBeNull();

    // (2a) Switched away (hidden by the host) — TabView keeps it mounted.
    fixture.componentRef.setInput('active', false);
    fixture.componentRef.setInput('visible', false);
    fixture.detectChanges();
    expect(constructions).toBe(1);
    expect(fixture.nativeElement.querySelector('[data-stub]')).toBe(stubEl); // same DOM node

    // Back again — still the same instance.
    fixture.componentRef.setInput('active', true);
    fixture.componentRef.setInput('visible', true);
    fixture.detectChanges();
    expect(constructions).toBe(1);

    // (2b) A fresh state object with the SAME tab id (a reducer payload refresh)
    // must not recreate the view.
    fixture.componentRef.setInput('tab', { ...homeTab(), title: 'Home (refreshed)' });
    fixture.detectChanges();
    expect(constructions).toBe(1);
    expect(fixture.nativeElement.querySelector('[data-stub]')).toBe(stubEl);
  });
});
