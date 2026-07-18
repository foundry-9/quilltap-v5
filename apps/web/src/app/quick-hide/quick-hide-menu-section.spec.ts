import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { TagDto } from '../core/core-contract';
import { QuickHideIcon, QuickHideMenuSection } from './quick-hide-menu-section';
import { QuickHideService } from './quick-hide.service';
import { ACTIVE_TAGS_KEY } from './quick-hide.storage';

/**
 * The toggle surface (v4 `nav-user-menu-quick-hide.tsx`). Copy and icon
 * polarity are the whole point of this component, so both are pinned here.
 */

function stubStorage(seed: Record<string, string> = {}): void {
  const store = new Map<string, string>(Object.entries(seed));
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
}

async function render(tags: TagDto[]): Promise<ComponentFixture<QuickHideMenuSection>> {
  const client: Partial<CoreClient> = {
    dispatchData: (async () => ({ tags })) as CoreClient['dispatchData'],
  };
  TestBed.configureTestingModule({
    imports: [QuickHideMenuSection],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(QuickHideMenuSection);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function buttonByText(fixture: ComponentFixture<unknown>, text: string): HTMLButtonElement {
  const buttons = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
  ) as HTMLButtonElement[];
  const found = buttons.find((b) => (b.textContent ?? '').includes(text));
  if (!found) throw new Error(`no button containing "${text}"`);
  return found;
}

/**
 * Which eye variant a button is showing. `qt-icon` renders the name onto an
 * inner span as `data-icon` (`ui/icon.ts:34-36`), not onto the host element.
 */
function iconName(root: HTMLElement): string | null {
  return root.querySelector('[data-icon]')?.getAttribute('data-icon') ?? null;
}

describe('QuickHideMenuSection (v4 nav-user-menu-quick-hide.tsx)', () => {
  beforeEach(() => TestBed.resetTestingModule());
  afterEach(() => vi.unstubAllGlobals());

  it('renders the two section labels verbatim (v4 :64,:87)', async () => {
    stubStorage();
    const fixture = await render([{ id: 'a', name: 'Alpha', quickHide: true }]);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Quick Hide Tags');
    expect(text).toContain('Content Filters');
    expect(text).toContain('Dangerous Chats');
    expect(text).toContain('Show Autonomous Rooms');
  });

  it('omits the tags section entirely when no tag is flagged (v4 :62)', async () => {
    stubStorage();
    const fixture = await render([]);
    const text = fixture.nativeElement.textContent as string;
    expect(text).not.toContain('Quick Hide Tags');
    // The content filters still render.
    expect(text).toContain('Content Filters');
  });

  it('renders one button per flagged tag and toggles it (v4 :67-80)', async () => {
    stubStorage();
    const fixture = await render([
      { id: 'a', name: 'Alpha', quickHide: true },
      { id: 'b', name: 'Beta', quickHide: true },
    ]);
    const service = TestBed.inject(QuickHideService);

    buttonByText(fixture, 'Alpha').click();
    fixture.detectChanges();

    expect(service.shouldHideByIds(['a'])).toBe(true);
    expect(service.shouldHideByIds(['b'])).toBe(false);
  });

  it('marks a hidden tag active and flips its eye (v4 :74,:77)', async () => {
    stubStorage({ [ACTIVE_TAGS_KEY]: JSON.stringify(['a']) });
    const fixture = await render([{ id: 'a', name: 'Alpha', quickHide: true }]);
    const button = buttonByText(fixture, 'Alpha');
    expect(button.classList.contains('qt-navbar-dropdown-item-active')).toBe(true);
    expect(iconName(button)).toBe('eye-off');
  });

  it('the Dangerous Chats toggle hides with an eye-off (v4 :89-96)', async () => {
    stubStorage();
    const fixture = await render([]);
    const service = TestBed.inject(QuickHideService);
    const button = buttonByText(fixture, 'Dangerous Chats');

    expect(iconName(button)).toBe('eye');
    button.click();
    fixture.detectChanges();

    expect(service.hideDangerousChats()).toBe(true);
    expect(iconName(button)).toBe('eye-off');
    expect(button.classList.contains('qt-navbar-dropdown-item-active')).toBe(true);
  });

  it('the autonomous toggle has INVERTED icon polarity (v4 :105)', async () => {
    stubStorage();
    const fixture = await render([]);
    const service = TestBed.inject(QuickHideService);
    const button = buttonByText(fixture, 'Show Autonomous Rooms');

    // OFF = excluded = struck-through eye. This is the opposite of the two
    // "hide" toggles, because this one ADDS rows.
    expect(iconName(button)).toBe('eye-off');
    button.click();
    fixture.detectChanges();

    expect(service.includeAutonomousRooms()).toBe(true);
    expect(iconName(button)).toBe('eye');
  });

  it('carries v4’s title on the autonomous toggle (v4 :102)', async () => {
    stubStorage();
    const fixture = await render([]);
    expect(buttonByText(fixture, 'Show Autonomous Rooms').getAttribute('title')).toBe(
      'Show autonomous character-to-character rooms in the Salon chat list',
    );
  });

  it('emits visibilityChanged on every toggle (v4 :18,:44-57)', async () => {
    stubStorage();
    const fixture = await render([{ id: 'a', name: 'Alpha', quickHide: true }]);
    let count = 0;
    fixture.componentInstance.visibilityChanged.subscribe(() => (count += 1));

    buttonByText(fixture, 'Alpha').click();
    buttonByText(fixture, 'Dangerous Chats').click();
    buttonByText(fixture, 'Show Autonomous Rooms').click();
    fixture.detectChanges();

    expect(count).toBe(3);
  });
});

describe('QuickHideIcon (v4 nav-user-menu-quick-hide.tsx:116)', () => {
  beforeEach(() => TestBed.resetTestingModule());

  it('is an open eye when nothing is hidden and struck-through when something is', () => {
    TestBed.configureTestingModule({ imports: [QuickHideIcon] });
    const fixture = TestBed.createComponent(QuickHideIcon);
    fixture.componentRef.setInput('hasHidden', false);
    fixture.detectChanges();
    expect(iconName(fixture.nativeElement)).toBe('eye');

    fixture.componentRef.setInput('hasHidden', true);
    fixture.detectChanges();
    expect(iconName(fixture.nativeElement)).toBe('eye-off');
  });
});
