import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { CONTENT_WIDTH_STORAGE_KEY } from './content-width.service';
import { NavContentWidthToggle } from './nav-content-width-toggle';

type MqListener = (e: { matches: boolean }) => void;

function installMatchMedia(matches: boolean): void {
  const mql = {
    matches,
    addEventListener: (_t: string, _l: MqListener) => undefined,
    removeEventListener: () => undefined,
  };
  (window as unknown as { matchMedia: () => typeof mql }).matchMedia = () => mql;
}

function reset(): void {
  localStorage.clear();
  document.documentElement.removeAttribute('data-full-width');
}

function render() {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [NavContentWidthToggle] });
  const fixture = TestBed.createComponent(NavContentWidthToggle);
  fixture.detectChanges();
  return fixture;
}

describe('NavContentWidthToggle (v4 nav-content-width-toggle.tsx)', () => {
  beforeEach(reset);
  afterEach(() => {
    reset();
    delete (window as { matchMedia?: unknown }).matchMedia;
  });

  it('renders nothing when the viewport is too narrow (v4 null return)', () => {
    installMatchMedia(false);
    const fixture = render();
    expect(fixture.nativeElement.querySelector('button')).toBeNull();
  });

  it('renders the expand icon + labels in narrow mode, aria-pressed=false', () => {
    installMatchMedia(true);
    const fixture = render();
    const button = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    expect(button).not.toBeNull();
    expect(button.classList.contains('qt-navbar-toggle')).toBe(true);
    expect(button.classList.contains('qt-navbar-toggle-active')).toBe(false);
    expect(button.title).toBe('Switch to wide layout');
    expect(button.getAttribute('aria-pressed')).toBe('false');
  });

  it('click toggles wide: active class, compress labels, persisted key', () => {
    installMatchMedia(true);
    const fixture = render();
    const button = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    button.click();
    fixture.detectChanges();
    expect(button.classList.contains('qt-navbar-toggle-active')).toBe(true);
    expect(button.title).toBe('Switch to narrow layout');
    expect(button.getAttribute('aria-pressed')).toBe('true');
    expect(localStorage.getItem(CONTENT_WIDTH_STORAGE_KEY)).toBe('true');
    expect(document.documentElement.getAttribute('data-full-width')).toBe('true');
  });
});
