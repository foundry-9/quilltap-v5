import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { ScriptoriumBadge, type ScriptoriumStatus } from './scriptorium-badge';

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
  return { fixture, get renders() { return counts.renders; } };
}

function button(fixture: ComponentFixture<ScriptoriumBadge>): HTMLButtonElement {
  return (fixture.nativeElement as HTMLElement).querySelector('button')!;
}

describe('ScriptoriumBadge', () => {
  it('renders the not-rendered state (destructive, click-to-render title)', async () => {
    const view = await mount('none');
    const btn = button(view.fixture);
    expect(btn.className).toContain('qt-text-destructive');
    expect(btn.title).toBe('Scriptorium: Not yet rendered — click to render');
  });

  it('renders the rendered state (warning, re-render title)', async () => {
    const view = await mount('rendered');
    const btn = button(view.fixture);
    expect(btn.className).toContain('qt-text-warning');
    expect(btn.title).toBe('Scriptorium: Rendered but not fully embedded — click to re-render');
  });

  it('renders the embedded state (success, re-render title)', async () => {
    const view = await mount('embedded');
    const btn = button(view.fixture);
    expect(btn.className).toContain('qt-text-success');
    expect(btn.title).toBe('Scriptorium: Rendered and embedded — click to re-render');
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
