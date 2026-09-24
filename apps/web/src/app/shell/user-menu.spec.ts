import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { UserMenu } from './user-menu';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

/** Tags the stubbed server reports as flagged `quickHide`, per test. */
let quickHideTags: Array<Record<string, unknown>> = [];
/** Every dispatched verb, in order — reset per render. */
let dispatchedTypes: string[] = [];

function stubClient(profile: Record<string, unknown> | null): Partial<CoreClient> {
  dispatchedTypes = [];
  return {
    dispatchData: (async (req: DispatchReq) => {
      dispatchedTypes.push(req.type);
      if (req.type === 'userProfileGet') {
        return { profile: profile ?? {} };
      }
      if (req.type === 'tagList') {
        return { tags: quickHideTags };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  profile: Record<string, unknown> | null,
): Promise<ComponentFixture<UserMenu>> {
  TestBed.configureTestingModule({
    imports: [UserMenu],
    providers: [
      { provide: CoreClient, useValue: stubClient(profile) },
      provideRouter([]),
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
    ],
  });
  const fixture = TestBed.createComponent(UserMenu);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function trigger(fixture: ComponentFixture<UserMenu>): HTMLButtonElement {
  return (fixture.nativeElement as HTMLElement).querySelector<HTMLButtonElement>(
    'button[aria-label="User menu"]',
  )!;
}

describe('UserMenu (v4 components/layout/left-sidebar/profile-menu.tsx)', () => {
  afterEach(() => {
    quickHideTags = [];
    TestBed.resetTestingModule();
  });

  it('is closed until the trigger is pressed', async () => {
    const fixture = await render({ id: 'u1', username: 'friday', name: 'Friday' });
    expect(trigger(fixture).getAttribute('aria-expanded')).toBe('false');
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="menu"]')).toBeNull();

    trigger(fixture).click();
    fixture.detectChanges();

    expect(trigger(fixture).getAttribute('aria-expanded')).toBe('true');
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="menu"]')).not.toBeNull();
  });

  it('shows the name and email from userProfileGet, with v4 fallbacks', async () => {
    const fixture = await render({
      id: 'u1',
      username: 'friday',
      name: 'Friday',
      email: 'friday@foundry-9.com',
    });
    trigger(fixture).click();
    fixture.detectChanges();

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Friday');
    expect(text).toContain('friday@foundry-9.com');
  });

  it("falls back to 'User' when the profile has no name (v4 :81)", async () => {
    const fixture = await render({ id: 'u1', username: 'friday' });
    trigger(fixture).click();
    fixture.detectChanges();

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('User');
  });

  it('navigates to /profile and /about, closing the menu each time', async () => {
    const fixture = await render({ id: 'u1', username: 'friday', name: 'Friday' });
    const router = TestBed.inject(Router);
    const navigate = vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);

    const entries = () =>
      Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>(
          '[role="menuitem"]',
        ),
      );

    trigger(fixture).click();
    fixture.detectChanges();
    entries().find((b) => b.textContent?.includes('Profile'))!.click();
    fixture.detectChanges();
    expect(navigate).toHaveBeenCalledWith('/profile');
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="menu"]')).toBeNull();

    trigger(fixture).click();
    fixture.detectChanges();
    entries().find((b) => b.textContent?.includes('About'))!.click();
    fixture.detectChanges();
    expect(navigate).toHaveBeenCalledWith('/about');
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="menu"]')).toBeNull();
  });

  it('renders the avatar image when the profile carries one', async () => {
    const fixture = await render({
      id: 'u1',
      username: 'friday',
      name: 'Friday',
      image: '/api/v1/files/abc',
    });
    const img = (fixture.nativeElement as HTMLElement).querySelector('img');
    expect(img?.getAttribute('src')).toContain('/api/v1/files/abc');
  });
});

/**
 * The quick-hide section — ALWAYS offered since v4 `e3937d7aa`
 * (`sidebar-footer.tsx:142-143`: "The Salon Images switch is always
 * meaningful, so the quick-hide button is always offered" —
 * `hasQuickHideFeatures = mounted`). The gate v5 used to carry (a flagged tag,
 * the danger toggle, or the `chatsHasDangerous` probe) retired with the probe.
 */
describe('UserMenu — the quick-hide section is always offered (v4 e3937d7aa sidebar-footer.tsx:143)', () => {
  afterEach(() => {
    quickHideTags = [];
    window.localStorage.clear();
    TestBed.resetTestingModule();
  });

  const section = (fixture: ComponentFixture<UserMenu>): Element | null =>
    (fixture.nativeElement as HTMLElement).querySelector('qt-quick-hide-menu-section');

  it('offers the section with no flagged tag and nothing hidden', async () => {
    window.localStorage.clear();
    const fixture = await render({ id: 'u1', username: 'friday' });
    trigger(fixture).click();
    fixture.detectChanges();
    await settle(fixture);
    expect(section(fixture)).not.toBeNull();
    // The Salon Images switch is the reason — it is always meaningful.
    expect((section(fixture)!.textContent ?? '')).toContain('Salon Images');
  });

  it('offers the section once a tag is flagged for quick-hide', async () => {
    window.localStorage.clear();
    quickHideTags = [{ id: 't1', name: 'Spicy', quickHide: true }];
    const fixture = await render({ id: 'u1', username: 'friday' });
    trigger(fixture).click();
    fixture.detectChanges();
    await settle(fixture);
    expect(section(fixture)).not.toBeNull();
  });

  it('offers the section when "Dangerous Chats" is already on', async () => {
    window.localStorage.clear();
    window.localStorage.setItem('quilltap.quickHide.hideDangerous', 'true');
    const fixture = await render({ id: 'u1', username: 'friday' });
    trigger(fixture).click();
    fixture.detectChanges();
    await settle(fixture);
    expect(section(fixture)).not.toBeNull();
  });

  it('never dispatches the retired chatsHasDangerous probe', async () => {
    window.localStorage.clear();
    const fixture = await render({ id: 'u1', username: 'friday' });
    trigger(fixture).click();
    fixture.detectChanges();
    await settle(fixture);
    expect(dispatchedTypes).not.toContain('chatsHasDangerous');
  });
});
