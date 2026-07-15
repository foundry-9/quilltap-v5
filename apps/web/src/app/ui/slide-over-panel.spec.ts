import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { afterEach, describe, expect, it } from 'vitest';

import { SlideOverPanel } from './slide-over-panel';

/**
 * A host so the panel can be driven the way its consumers drive it (a bound
 * `isOpen` signal + projected header actions + body content).
 */
@Component({
  imports: [SlideOverPanel],
  template: `
    <qt-slide-over-panel
      [isOpen]="open()"
      [title]="title()"
      [ariaLabel]="ariaLabel()"
      [width]="width()"
      (close)="closed = closed + 1"
    >
      <div qt-slide-over-actions>
        <button type="button" id="action">Refresh</button>
      </div>
      <div id="body"><button type="button" id="inner">Inner</button></div>
    </qt-slide-over-panel>
  `,
})
class Host {
  readonly open = signal(false);
  readonly title = signal('LLM Inspector');
  readonly ariaLabel = signal<string | undefined>(undefined);
  readonly width = signal('min(480px, 85vw)');
  closed = 0;
}

function render(): ComponentFixture<Host> {
  // Reset first so a test may render more than once (TestBed refuses to
  // reconfigure an instantiated module).
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return fixture;
}

function scrim(fixture: ComponentFixture<Host>): HTMLElement {
  return fixture.nativeElement.querySelector('.qt-slide-over-overlay');
}
function panel(fixture: ComponentFixture<Host>): HTMLElement {
  return fixture.nativeElement.querySelector('.qt-slide-over-panel');
}

describe('SlideOverPanel — the data-open contract (v4 SlideOverPanel.tsx:104-121)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('mounts the scrim and panel even when CLOSED — the CSS transitions animate on data-open', () => {
    // An @if here would replace v4's slide-in with a hard cut
    // (`_surfaces.css:1067-1102` keys every transition off the attribute).
    const fixture = render();
    expect(scrim(fixture)).not.toBeNull();
    expect(panel(fixture)).not.toBeNull();
    expect(scrim(fixture).getAttribute('data-open')).toBe('false');
    expect(panel(fixture).getAttribute('data-open')).toBe('false');
  });

  it('flips data-open on both elements when opened', () => {
    const fixture = render();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    expect(scrim(fixture).getAttribute('data-open')).toBe('true');
    expect(panel(fixture).getAttribute('data-open')).toBe('true');
  });
});

describe('SlideOverPanel — dialog semantics (v4 :112-122)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('carries v4’s role/aria-modal/tabindex', () => {
    const fixture = render();
    const p = panel(fixture);
    expect(p.getAttribute('role')).toBe('dialog');
    expect(p.getAttribute('aria-modal')).toBe('true');
    expect(p.getAttribute('tabindex')).toBe('-1');
  });

  it('defaults width to v4’s prop default (v4 :30)', () => {
    // Asserted at the INPUT, not the rendered style: jsdom's CSS parser has no
    // min() support, so it silently drops the declaration and `style.width`
    // reads ''. Real browsers apply it — the e2e walk is what exercises the
    // rendered panel.
    const fixture = render();
    const panelCmp = fixture.debugElement.query(By.directive(SlideOverPanel))
      .componentInstance as SlideOverPanel;
    expect(panelCmp.width()).toBe('min(480px, 85vw)');
  });

  it('applies a width the CSS parser accepts', () => {
    // Proves the [style.width] binding is live despite the min() case above.
    const fixture = render();
    fixture.componentInstance.width.set('640px');
    fixture.detectChanges();
    expect(panel(fixture).style.width).toBe('640px');
  });

  it('falls back to the title for aria-label, and prefers ariaLabel when given (v4 :119)', () => {
    const fixture = render();
    expect(panel(fixture).getAttribute('aria-label')).toBe('LLM Inspector');
    fixture.componentInstance.ariaLabel.set('LLM Inspector Panel');
    fixture.detectChanges();
    expect(panel(fixture).getAttribute('aria-label')).toBe('LLM Inspector Panel');
  });

  it('renders the title and marks the scrim aria-hidden', () => {
    const fixture = render();
    expect(fixture.nativeElement.querySelector('.qt-slide-over-header h2').textContent.trim()).toBe(
      'LLM Inspector',
    );
    expect(scrim(fixture).getAttribute('aria-hidden')).toBe('true');
  });

  it('projects header actions BEFORE the close button (v4 :126-134)', () => {
    const fixture = render();
    const header = fixture.nativeElement.querySelector('.qt-slide-over-header');
    const buttons = Array.from(header.querySelectorAll('button')) as HTMLButtonElement[];
    expect(buttons[0].id).toBe('action');
    expect(buttons[buttons.length - 1].getAttribute('aria-label')).toBe('Close panel');
  });
});

describe('SlideOverPanel — closing (v4 :53-72, :127-134)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('closes on the ✕ button', () => {
    const fixture = render();
    (
      fixture.nativeElement.querySelector('button[aria-label="Close panel"]') as HTMLButtonElement
    ).click();
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('closes on a scrim click', () => {
    const fixture = render();
    scrim(fixture).dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('does NOT close when a click bubbles out of the panel to the scrim (v4 :69 target check)', () => {
    const fixture = render();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    // Simulate a bubbled click: the listener runs with a target that is not the
    // scrim itself.
    const event = new MouseEvent('click', { bubbles: true });
    Object.defineProperty(event, 'target', { value: panel(fixture) });
    scrim(fixture).dispatchEvent(event);
    expect(fixture.componentInstance.closed).toBe(0);
  });

  it('closes on Escape only while OPEN (v4 attaches the listener under `if (!isOpen) return`)', () => {
    const fixture = render();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(fixture.componentInstance.closed).toBe(0);

    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('detaches the Escape listener when it closes again', () => {
    const fixture = render();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    fixture.componentInstance.open.set(false);
    fixture.detectChanges();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(fixture.componentInstance.closed).toBe(0);
  });

  it('ignores other keys', () => {
    const fixture = render();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(fixture.componentInstance.closed).toBe(0);
  });
});

describe('SlideOverPanel — the focus trap (v4 :75-97)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('wraps Tab from the last focusable back to the first', () => {
    const fixture = render();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    const p = panel(fixture);
    const focusables = Array.from(p.querySelectorAll('button')) as HTMLButtonElement[];
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    last.focus();

    const event = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    p.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);
  });

  it('wraps Shift+Tab from the first focusable to the last', () => {
    const fixture = render();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    const p = panel(fixture);
    const focusables = Array.from(p.querySelectorAll('button')) as HTMLButtonElement[];
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    first.focus();

    const event = new KeyboardEvent('keydown', {
      key: 'Tab',
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    });
    p.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(last);
  });

  it('leaves a mid-cycle Tab to the browser', () => {
    const fixture = render();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    const p = panel(fixture);
    const focusables = Array.from(p.querySelectorAll('button')) as HTMLButtonElement[];
    // Focus something that is neither first nor last.
    focusables[1].focus();
    const event = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    p.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });
});

describe('SlideOverPanel — focus save/restore (v4 :39-50)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('returns focus to the element that was focused before opening', async () => {
    const fixture = render();
    const opener = document.createElement('button');
    document.body.appendChild(opener);
    opener.focus();
    expect(document.activeElement).toBe(opener);

    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    // The panel takes focus on the next frame (v4's requestAnimationFrame).
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    expect(document.activeElement).toBe(panel(fixture));

    fixture.componentInstance.open.set(false);
    fixture.detectChanges();
    expect(document.activeElement).toBe(opener);

    opener.remove();
  });
});
