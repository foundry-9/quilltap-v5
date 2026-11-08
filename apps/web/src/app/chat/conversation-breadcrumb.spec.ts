import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { BreadcrumbDensity } from './breadcrumb-density';
import {
  BreadcrumbBadge,
  ConversationBreadcrumb,
  type BreadcrumbCharacter,
  type BreadcrumbStoryBackground,
} from './conversation-breadcrumb';

/**
 * v4 `components/chat/ChatHeaderBreadcrumb.tsx` (`3c41cf3da`, pinned at
 * `624d0d25c`) in Angular: the three densities' DOM and aria, the compact
 * tooltips' words, the inert measuring copies, and the `ResizeObserver` that
 * drives the choice (stubbed — jsdom has none and lays nothing out).
 */

/** A recording `ResizeObserver` stand-in; `fire()` runs every live callback. */
class FakeResizeObserver {
  static instances: FakeResizeObserver[] = [];
  readonly observed: Element[] = [];
  disconnected = false;
  constructor(private readonly callback: () => void) {
    FakeResizeObserver.instances.push(this);
  }
  observe(el: Element): void {
    this.observed.push(el);
  }
  unobserve(): void {}
  disconnect(): void {
    this.disconnected = true;
  }
  static fire(): void {
    for (const o of FakeResizeObserver.instances) if (!o.disconnected) o.callback();
  }
}

/** The room the host reports, and each measuring copy's natural width. */
const layout = {
  available: 1000,
  widths: { full: 600, abbrev: 400, avatar: 250 } as Record<BreadcrumbDensity, number>,
};
const ORDER: BreadcrumbDensity[] = ['full', 'abbrev', 'avatar'];

@Component({
  imports: [ConversationBreadcrumb, BreadcrumbBadge],
  template: `
    <qt-conversation-breadcrumb
      chatId="chat-1"
      [title]="title()"
      [project]="project()"
      [characters]="characters()"
      [storyBackground]="background()"
      (openStoryBackground)="opened.push($event)"
    >
      <ng-template qtBreadcrumbBadge>
        <span class="test-badge" role="img" aria-label="Concierge: Unmoderated">Unmoderated</span>
      </ng-template>
    </qt-conversation-breadcrumb>
  `,
})
class Host {
  readonly title = signal('A Long Night at the Brass Owl');
  readonly project = signal<{ id: string; name: string } | null>({
    id: 'proj-1',
    name: 'Voyages of the Covenant',
  });
  readonly characters = signal<BreadcrumbCharacter[]>([
    { id: 'char-1', name: 'Abigail', avatarUrl: '/api/v1/images/abigail.png' },
    { id: 'char-2', name: 'Baraka Ilunga', avatarUrl: null },
  ]);
  readonly background = signal<BreadcrumbStoryBackground | null>({
    src: '/api/v1/files/bg-1',
    filename: 'bg.png',
    fileId: 'bg-1',
  });
  readonly opened: BreadcrumbStoryBackground[] = [];
}

describe('ConversationBreadcrumb (v4 ChatHeaderBreadcrumb @ 624d0d25c)', () => {
  let clientWidth: ReturnType<typeof vi.spyOn>;
  let scrollWidth: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    FakeResizeObserver.instances = [];
    vi.stubGlobal('ResizeObserver', FakeResizeObserver);
    layout.available = 1000;
    layout.widths = { full: 600, abbrev: 400, avatar: 250 };
    clientWidth = vi
      .spyOn(HTMLElement.prototype, 'clientWidth', 'get')
      .mockImplementation(function (this: HTMLElement) {
        return this.tagName === 'QT-CONVERSATION-BREADCRUMB' ? layout.available : 0;
      });
    scrollWidth = vi
      .spyOn(HTMLElement.prototype, 'scrollWidth', 'get')
      .mockImplementation(function (this: HTMLElement) {
        const host = this.parentElement;
        if (!host || host.tagName !== 'QT-CONVERSATION-BREADCRUMB' || !this.hasAttribute('inert')) {
          return 0;
        }
        const copies = Array.from(host.querySelectorAll(':scope > [inert]'));
        return layout.widths[ORDER[copies.indexOf(this)]];
      });
    TestBed.configureTestingModule({ imports: [Host], providers: [provideRouter([])] });
  });

  afterEach(() => {
    clientWidth.mockRestore();
    scrollWidth.mockRestore();
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
    for (const el of Array.from(document.body.querySelectorAll('.qt-tooltip'))) el.remove();
  });

  async function render(): Promise<ComponentFixture<Host>> {
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return fixture;
  }

  async function resizeTo(fixture: ComponentFixture<Host>, available: number): Promise<void> {
    layout.available = available;
    FakeResizeObserver.fire();
    fixture.detectChanges();
    await fixture.whenStable();
  }

  const crumb = (f: ComponentFixture<Host>): HTMLElement =>
    f.nativeElement.querySelector('qt-conversation-breadcrumb');
  const copies = (f: ComponentFixture<Host>): HTMLElement[] =>
    Array.from(crumb(f).querySelectorAll(':scope > [inert]')) as HTMLElement[];
  /** Matches in the VISIBLE row only (outside the measuring copies). */
  const visible = (f: ComponentFixture<Host>, sel: string): HTMLElement[] =>
    (Array.from(crumb(f).querySelectorAll(sel)) as HTMLElement[]).filter(
      (el) => el.closest('[inert]') === null,
    );
  const one = (f: ComponentFixture<Host>, sel: string): HTMLElement => {
    const [el, ...rest] = visible(f, sel);
    expect(el, sel).toBeTruthy();
    expect(rest).toHaveLength(0);
    return el;
  };
  const projectLink = (f: ComponentFixture<Host>) => one(f, 'a[href="/prospero/proj-1"]');
  const abigail = (f: ComponentFixture<Host>) =>
    one(f, 'a[href="/characters/char-1?tab=conversations"]');
  const baraka = (f: ComponentFixture<Host>) =>
    one(f, 'a[href="/characters/char-2?tab=conversations"]');

  async function openTip(fixture: ComponentFixture<Host>, anchor: HTMLElement): Promise<string> {
    // focusin opens at once (v4's onFocus) — no hover delay to wait out.
    anchor.closest('qt-tooltip')!.dispatchEvent(new Event('focusin', { bubbles: true }));
    fixture.detectChanges();
    await fixture.whenStable();
    return document.body.querySelector('.qt-tooltip')?.textContent ?? '';
  }

  it("carries v4's container classes on the host", async () => {
    const fixture = await render();
    expect(crumb(fixture).className).toBe(
      'hidden md:flex relative items-center gap-2 text-sm min-w-0 w-full overflow-hidden',
    );
  });

  describe('full density', () => {
    it('shows every name, no aria-labels and no compact tooltips', async () => {
      const fixture = await render();
      const project = projectLink(fixture);
      expect(project.getAttribute('aria-label')).toBeNull();
      expect(project.textContent?.trim()).toBe('Voyages of the Covenant');
      expect(project.querySelector('qt-icon')?.getAttribute('name')).toBe('folder');
      expect(project.closest('qt-tooltip')).toBeNull();
      expect(project.className).toBe(
        'inline-flex items-center gap-1.5 qt-text-secondary hover:text-foreground transition-colors flex-shrink-0',
      );

      expect(abigail(fixture).getAttribute('aria-label')).toBeNull();
      expect(abigail(fixture).textContent?.trim()).toBe('Abigail');
      expect(abigail(fixture).closest('qt-tooltip')).toBeNull();
      expect(baraka(fixture).querySelector(':scope > span:not(.qt-bg-muted)')!.textContent).toBe(
        'Baraka Ilunga',
      );
    });

    it('draws the avatar image, or the initial disc when there is none', async () => {
      const fixture = await render();
      const img = abigail(fixture).querySelector('img')!;
      expect(img.getAttribute('src')).toBe('/api/v1/images/abigail.png');
      expect(img.getAttribute('alt')).toBe('');
      expect(img.className).toBe('w-5 h-5 rounded-full object-cover flex-shrink-0');
      expect(baraka(fixture).querySelector('img')).toBeNull();
      const disc = baraka(fixture).querySelector('span.qt-bg-muted')!;
      expect(disc.textContent?.trim()).toBe('B');
      expect(disc.querySelector('.qt-text-secondary')).not.toBeNull();
    });

    it('separates each project / character link with a muted slash', async () => {
      const fixture = await render();
      expect(visible(fixture, 'span.qt-text-muted').map((s) => s.textContent)).toEqual([
        '/',
        '/',
        '/',
      ]);
    });

    it('omits the clipping group when there is no project and no character', async () => {
      const fixture = TestBed.createComponent(Host);
      fixture.componentInstance.project.set(null);
      fixture.componentInstance.characters.set([]);
      fixture.detectChanges();
      await fixture.whenStable();
      expect(visible(fixture, 'span.overflow-hidden')).toHaveLength(0);
      expect(visible(fixture, 'a[href="/salon/chat-1"]')).toHaveLength(1);
    });
  });

  describe('a stubbed ResizeObserver drives the density', () => {
    it('picks full when it fits, abbrev when only it fits, avatar below that', async () => {
      const fixture = await render();
      expect(projectLink(fixture).textContent?.trim()).toBe('Voyages of the Covenant');

      await resizeTo(fixture, 450);
      expect(projectLink(fixture).getAttribute('aria-label')).toBe(
        'Project: Voyages of the Covenant',
      );
      const short = projectLink(fixture).querySelector(':scope > span[aria-hidden="true"]')!;
      expect(short.textContent).toBe('VC');
      expect(abigail(fixture).getAttribute('aria-label')).toBe('Abigail');
      expect(abigail(fixture).querySelector(':scope > span[aria-hidden="true"]')!.textContent).toBe('Abi');
      expect(baraka(fixture).querySelector(':scope > span[aria-hidden="true"]')!.textContent).toBe('BI');

      await resizeTo(fixture, 100);
      expect(projectLink(fixture).getAttribute('aria-label')).toBe(
        'Project: Voyages of the Covenant',
      );
      expect(projectLink(fixture).querySelector(':scope > span')).toBeNull();
      expect(abigail(fixture).querySelectorAll(':scope > span')).toHaveLength(0);
      expect(abigail(fixture).querySelector('img')).not.toBeNull();

      await resizeTo(fixture, 600);
      expect(projectLink(fixture).getAttribute('aria-label')).toBeNull();
    });

    it('recomputes when a copy changes width, not only the container', async () => {
      const fixture = await render();
      layout.widths = { full: 1200, abbrev: 900, avatar: 300 };
      FakeResizeObserver.fire();
      fixture.detectChanges();
      await fixture.whenStable();
      expect(projectLink(fixture).querySelector(':scope > span[aria-hidden="true"]')!.textContent).toBe(
        'VC',
      );
    });

    it('does NOT recompute while the container reports 0 (hidden below md)', async () => {
      const fixture = await render();
      await resizeTo(fixture, 100);
      expect(projectLink(fixture).querySelector(':scope > span')).toBeNull();
      await resizeTo(fixture, 0);
      // Still avatar — a 0 would otherwise have chosen avatar anyway, so prove it
      // the other way: widen the copies' needs to nothing and stay put.
      layout.widths = { full: 0, abbrev: 0, avatar: 0 };
      FakeResizeObserver.fire();
      fixture.detectChanges();
      await fixture.whenStable();
      expect(projectLink(fixture).querySelector(':scope > span')).toBeNull();
    });

    it('observes the container and the three copies, and disconnects on destroy', async () => {
      const fixture = await render();
      expect(FakeResizeObserver.instances).toHaveLength(1);
      const [observer] = FakeResizeObserver.instances;
      expect(observer.observed[0]).toBe(crumb(fixture));
      expect(observer.observed.slice(1)).toEqual(copies(fixture));
      expect(observer.observed).toHaveLength(4);
      expect(observer.disconnected).toBe(false);
      fixture.destroy();
      expect(observer.disconnected).toBe(true);
    });
  });

  describe('the compact tooltips', () => {
    it('names the project with "Click to open project"', async () => {
      const fixture = await render();
      await resizeTo(fixture, 450);
      const tip = projectLink(fixture).closest('qt-tooltip')!;
      expect(tip.classList.contains('flex-shrink-0')).toBe(true);
      const text = await openTip(fixture, projectLink(fixture));
      const bubble = document.body.querySelector('.qt-tooltip')!;
      expect(bubble.querySelector('.qt-tooltip-body .qt-tooltip-title')!.textContent).toBe(
        'Voyages of the Covenant',
      );
      expect(bubble.querySelector('.qt-tooltip-body .qt-tooltip-hint')!.textContent).toBe(
        'Click to open project',
      );
      expect(text).toContain('Voyages of the Covenant');
      expect(bubble.getAttribute('data-placement')).toBe('bottom');
    });

    it('names a character with "Click to open character"', async () => {
      const fixture = await render();
      await resizeTo(fixture, 100);
      await openTip(fixture, baraka(fixture));
      const bubble = document.body.querySelector('.qt-tooltip')!;
      expect(bubble.querySelector('.qt-tooltip-title')!.textContent).toBe('Baraka Ilunga');
      expect(bubble.querySelector('.qt-tooltip-hint')!.textContent).toBe(
        'Click to open character',
      );
    });
  });

  describe('the story background, the badge, the title and the copy-id button', () => {
    it('renders the thumbnail button with its tooltip, and emits the background on click', async () => {
      const fixture = await render();
      const button = one(fixture, 'button[aria-label="Display story background"]');
      const img = button.querySelector('img')!;
      expect(img.getAttribute('src')).toBe('/api/v1/files/bg-1');
      expect(img.className).toBe('w-8 h-5 object-cover');
      expect(button.closest('qt-tooltip')!.classList.contains('flex-shrink-0')).toBe(true);
      expect(await openTip(fixture, button)).toContain('Click to display background');
      button.click();
      expect(fixture.componentInstance.opened).toEqual([
        { src: '/api/v1/files/bg-1', filename: 'bg.png', fileId: 'bg-1' },
      ]);
    });

    it('omits the thumbnail when no background is visible', async () => {
      const fixture = TestBed.createComponent(Host);
      fixture.componentInstance.background.set(null);
      fixture.detectChanges();
      await fixture.whenStable();
      expect(crumb(fixture).querySelector('button[aria-label="Display story background"]')).toBeNull();
    });

    it('wraps the title in a tooltip anchored min-w-32 shrink-[1000]', async () => {
      const fixture = await render();
      const title = one(fixture, 'a[href="/salon/chat-1"]');
      expect(title.textContent).toBe('A Long Night at the Brass Owl');
      expect(title.className).toBe(
        'qt-text-primary truncate min-w-0 hover:text-foreground transition-colors',
      );
      expect(title.hasAttribute('title')).toBe(false);
      const tip = title.closest('qt-tooltip')!;
      expect(tip.classList.contains('min-w-32')).toBe(true);
      expect(tip.classList.contains('shrink-[1000]')).toBe(true);
      expect(await openTip(fixture, title)).toContain('A Long Night at the Brass Owl');
    });

    it('places the projected badge between the thumbnail and the title, and copy-id LAST', async () => {
      const fixture = await render();
      const badge = one(fixture, '.test-badge');
      const background = one(fixture, 'button[aria-label="Display story background"]');
      const title = one(fixture, 'a[href="/salon/chat-1"]');
      const copy = one(fixture, 'qt-copy-chat-id-button');
      const FOLLOWING = 4;
      expect(background.compareDocumentPosition(badge) & FOLLOWING).toBeTruthy();
      expect(badge.compareDocumentPosition(title) & FOLLOWING).toBeTruthy();
      expect(title.compareDocumentPosition(copy) & FOLLOWING).toBeTruthy();
      // LAST of the visible row: the next sibling is the first measuring copy.
      expect(copy.nextElementSibling?.hasAttribute('inert')).toBe(true);
      expect(copy.querySelector('button[aria-label="Copy conversation ID"]')).not.toBeNull();
    });
  });

  describe('the measuring copies', () => {
    it('are three, inert and aria-hidden, in full / abbrev / avatar order', async () => {
      const fixture = await render();
      const all = copies(fixture);
      expect(all).toHaveLength(3);
      for (const copy of all) {
        expect(copy.getAttribute('aria-hidden')).toBe('true');
        expect(copy.hasAttribute('inert')).toBe(true);
        expect(copy.className).toBe(
          'absolute left-0 top-0 flex items-center gap-2 w-max invisible pointer-events-none whitespace-nowrap',
        );
      }
      const project = (i: number) => all[i].querySelector('a[href="/prospero/proj-1"]')!;
      expect(project(0).textContent?.trim()).toBe('Voyages of the Covenant');
      expect(project(1).querySelector(':scope > span[aria-hidden="true"]')!.textContent).toBe('VC');
      expect(project(2).querySelector(':scope > span')).toBeNull();
    });

    it('carry no tooltips but the badge, take no focus, cap the title at 256 px', async () => {
      const fixture = await render();
      for (const copy of copies(fixture)) {
        expect(copy.querySelector('qt-tooltip')).toBeNull();
        expect(copy.querySelector('.test-badge')).not.toBeNull();
        for (const el of Array.from(copy.querySelectorAll('a, button[aria-label="Display story background"]'))) {
          expect(el.getAttribute('tabindex')).toBe('-1');
        }
        const title = copy.querySelector('a[href="/salon/chat-1"]') as HTMLElement;
        expect(title.style.maxWidth).toBe('256px');
        expect(title.style.display).toBe('inline-block');
      }
      // The visible row's links keep their natural tab order and width.
      expect(projectLink(fixture).hasAttribute('tabindex')).toBe(false);
      expect(one(fixture, 'a[href="/salon/chat-1"]').style.maxWidth).toBe('');
    });

    it("never emit from a copy's thumbnail", async () => {
      const fixture = await render();
      for (const copy of copies(fixture)) {
        (copy.querySelector('button[aria-label="Display story background"]') as HTMLElement).click();
      }
      expect(fixture.componentInstance.opened).toEqual([]);
    });
  });
});
