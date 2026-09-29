import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { describe, expect, it, vi } from 'vitest';

import { ScriptoriumBadge, type ScriptoriumStatus } from './scriptorium-badge';
import { Tooltip } from './tooltip';

/**
 * The shared Scriptorium badge (p4.9o) — the three-state colour/title logic
 * lifted from the character Conversations card, now clickable on both card
 * sites. Presentational: click emits `render` and swallows the link navigation.
 */

async function mount(
  status: ScriptoriumStatus,
  busy = false,
): Promise<{ fixture: ComponentFixture<ScriptoriumBadge>; renders: number }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ScriptoriumBadge] });
  const fixture = TestBed.createComponent(ScriptoriumBadge);
  const counts = { renders: 0 };
  fixture.componentInstance.render.subscribe(() => (counts.renders += 1));
  fixture.componentRef.setInput('status', status);
  fixture.componentRef.setInput('busy', busy);
  fixture.detectChanges();
  return {
    fixture,
    get renders() {
      return counts.renders;
    },
  };
}

function button(fixture: ComponentFixture<ScriptoriumBadge>): HTMLButtonElement {
  return (fixture.nativeElement as HTMLElement).querySelector('button')!;
}

/**
 * The tooltip string is v4's `SCRIPTORIUM_TOOLTIPS` at `f7f3d7bf0`
 * (`components/chat/ChatCard.tsx:98-103`), dashes U+2014: carried by the
 * `qt-tooltip` and repeated as the `aria-label`, with no native `title`.
 */
function expectTip(
  fixture: ComponentFixture<ScriptoriumBadge>,
  btn: HTMLButtonElement,
  text: string,
): void {
  const tip = fixture.debugElement.query(By.directive(Tooltip)).componentInstance as Tooltip;
  expect(tip.content()).toBe(text);
  expect(btn.getAttribute('aria-label')).toBe(text);
  expect(btn.hasAttribute('title')).toBe(false);
}

describe('ScriptoriumBadge', () => {
  it('renders the not-rendered state (destructive, click-to-render title)', async () => {
    const view = await mount('none');
    const btn = button(view.fixture);
    expect(btn.className).toContain('qt-text-destructive');
    expectTip(view.fixture, btn, 'Scriptorium: not yet transcribed — click to render and index');
  });

  it('renders the rendered state (warning, re-render title)', async () => {
    const view = await mount('rendered');
    const btn = button(view.fixture);
    expect(btn.className).toContain('qt-text-warning');
    expectTip(
      view.fixture,
      btn,
      'Scriptorium: transcribed, the indexing still under way — click to re-render',
    );
  });

  it('renders the embedded state (success, re-render title)', async () => {
    const view = await mount('embedded');
    const btn = button(view.fixture);
    expect(btn.className).toContain('qt-text-success');
    expectTip(
      view.fixture,
      btn,
      'Scriptorium: transcribed and indexed, every word findable — click to re-render',
    );
  });

  it('emits render and swallows the click (preventDefault/stopPropagation)', async () => {
    const view = await mount('none');
    const event = new MouseEvent('click', { bubbles: true, cancelable: true });
    const prevent = vi.spyOn(event, 'preventDefault');
    const stop = vi.spyOn(event, 'stopPropagation');
    button(view.fixture).dispatchEvent(event);
    expect(view.renders).toBe(1);
    expect(prevent).toHaveBeenCalled();
    expect(stop).toHaveBeenCalled();
  });

  it('disables the button while busy', async () => {
    const view = await mount('none', true);
    expect(button(view.fixture).disabled).toBe(true);
  });
});
