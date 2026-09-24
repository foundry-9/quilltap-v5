import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { TagDto } from '../core/core-contract';
import { QuickHideMenuSection } from './quick-hide-menu-section';
import { QuickHideService } from './quick-hide.service';
import { ACTIVE_TAGS_KEY, HIDE_SALON_IMAGES_KEY } from './quick-hide.storage';

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
    expect(text).toContain('Salon Images');
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

  it('the Salon Images toggle is the THIRD content filter, after Show Autonomous Rooms (v4 e3937d7aa :115-123)', async () => {
    stubStorage();
    const fixture = await render([]);
    const labels = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button span.text-sm'),
    ).map((el) => el.textContent?.trim());
    expect(labels).toEqual(['Dangerous Chats', 'Show Autonomous Rooms', 'Salon Images']);
    const button = buttonByText(fixture, 'Salon Images');
    expect(button.getAttribute('title')).toBe(
      'Hide backgrounds, avatars and attached images in the Salon',
    );
    expect(button.getAttribute('type')).toBe('button');
    expect(button.classList.contains('qt-navbar-dropdown-item')).toBe(true);
  });

  it('the Salon Images toggle hides with an eye-off — the Dangerous Chats polarity (v4 :122)', async () => {
    stubStorage();
    const fixture = await render([]);
    const service = TestBed.inject(QuickHideService);
    const button = buttonByText(fixture, 'Salon Images');

    expect(iconName(button)).toBe('eye');
    expect(button.classList.contains('qt-navbar-dropdown-item-active')).toBe(false);
    button.click();
    fixture.detectChanges();

    expect(service.hideSalonImages()).toBe(true);
    expect(iconName(button)).toBe('eye-off');
    expect(button.classList.contains('qt-navbar-dropdown-item-active')).toBe(true);
  });

  it('reads a stored Salon Images choice as active (v4 :117)', async () => {
    stubStorage({ [HIDE_SALON_IMAGES_KEY]: 'true' });
    const fixture = await render([]);
    const button = buttonByText(fixture, 'Salon Images');
    expect(button.classList.contains('qt-navbar-dropdown-item-active')).toBe(true);
    expect(iconName(button)).toBe('eye-off');
  });

  it('toggles Salon Images BEFORE notifying (v4 handleSalonImagesToggle :61-64)', async () => {
    stubStorage();
    const fixture = await render([]);
    const service = TestBed.inject(QuickHideService);
    const seenAtEmit: boolean[] = [];
    fixture.componentInstance.visibilityChanged.subscribe(() =>
      seenAtEmit.push(service.hideSalonImages()),
    );
    buttonByText(fixture, 'Salon Images').click();
    expect(seenAtEmit).toEqual([true]);
  });

  it('emits visibilityChanged on every toggle (v4 :18,:44-57)', async () => {
    stubStorage();
    const fixture = await render([{ id: 'a', name: 'Alpha', quickHide: true }]);
    let count = 0;
    fixture.componentInstance.visibilityChanged.subscribe(() => (count += 1));

    buttonByText(fixture, 'Alpha').click();
    buttonByText(fixture, 'Dangerous Chats').click();
    buttonByText(fixture, 'Show Autonomous Rooms').click();
    buttonByText(fixture, 'Salon Images').click();
    fixture.detectChanges();

    expect(count).toBe(4);
  });
});
