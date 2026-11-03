// v5-authored from v4 `components/characters/CanDrawDiagramsToggle.tsx` +
// `app/aurora/shared/save-diagram-settings.ts` (`6588972d5`, at 236c04e35) —
// v4 ships the card without a test. `choose-outfit-card.spec.ts` is the model.
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { CanDrawDiagramsCard } from './can-draw-diagrams-card';

function render(
  dispatchData: (req: unknown) => Promise<Record<string, unknown>>,
  inputs: Record<string, unknown> = {},
): { fixture: ComponentFixture<CanDrawDiagramsCard>; el: HTMLElement } {
  TestBed.configureTestingModule({
    imports: [CanDrawDiagramsCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: { dispatchData } }],
  });
  const fixture = TestBed.createComponent(CanDrawDiagramsCard);
  fixture.componentRef.setInput('characterId', 'c1');
  for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

const checkbox = (el: HTMLElement) => el.querySelector('input[type="checkbox"]') as HTMLInputElement;
const radio = (el: HTMLElement, value: string) =>
  el.querySelector(`input[type="radio"][value="${value}"]`) as HTMLInputElement;
const fieldset = (el: HTMLElement) => el.querySelector('fieldset') as HTMLFieldSetElement;

async function flush(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
}

function lastToast(): { type: string; message: string } | undefined {
  const t = TestBed.inject(ToastService).toasts().at(-1);
  return t && { type: t.type, message: t.message };
}

describe('CanDrawDiagramsCard (v4 6588972d5 Details-tab card)', () => {
  afterEach(() => vi.restoreAllMocks());

  it("v5: carries v4's label, hint, legend and radios", () => {
    const { el } = render(async () => ({}));
    expect(el.querySelector('[data-testid="can-draw-diagrams"]')).not.toBeNull();
    expect(el.textContent).toContain('Draws diagrams');
    const hint = el.querySelector('#can-draw-diagrams-hint')!;
    expect(hint.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'This character may sketch flowcharts, family trees, maps and the like right into the conversation. The toggle only grants the pen — to have them reach for it often, say so in their prompt.',
    );
    expect(checkbox(el).getAttribute('aria-describedby')).toBe('can-draw-diagrams-hint');
    expect(el.querySelector('legend')?.textContent?.trim()).toBe('Drawing style');
    const labels = Array.from(el.querySelectorAll('fieldset label')).map((l) => l.textContent?.replace(/\s+/g, ' ').trim());
    expect(labels).toEqual(['Hand-drawn — sketched in ink, a touch wobbly', 'Clean — ruled lines, as the engine drew them']);
    expect(radio(el, 'handDrawn').name).toBe('diagram-style');
    expect(radio(el, 'handDrawn').classList.contains('qt-radio')).toBe(true);
  });

  it("v5: tick → characterUpdate { canDrawDiagrams: true } and 'The pen is theirs'", async () => {
    const dispatchData = vi.fn(async () => ({}));
    const { el } = render(dispatchData);
    const cb = checkbox(el);
    cb.checked = true;
    cb.dispatchEvent(new Event('change'));
    await flush();
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'characterUpdate',
      characterId: 'c1',
      character: { canDrawDiagrams: true },
    });
    expect(lastToast()).toEqual({
      type: 'success',
      message: 'The pen is theirs — this character may now draw diagrams',
    });
  });

  it("v5: untick → { canDrawDiagrams: false } and 'The pen is put away'", async () => {
    const dispatchData = vi.fn(async () => ({}));
    const { el } = render(dispatchData, { canDrawDiagrams: true });
    const cb = checkbox(el);
    cb.checked = false;
    cb.dispatchEvent(new Event('change'));
    await flush();
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'characterUpdate',
      characterId: 'c1',
      character: { canDrawDiagrams: false },
    });
    expect(lastToast()?.message).toBe('The pen is put away — this character will draw no more diagrams');
  });

  it("v5: Clean → { diagramStyle: 'clean' } and 'drawn clean and ruled'; Hand-drawn → 'sketched by hand'", async () => {
    const dispatchData = vi.fn(async () => ({}));
    const { el, fixture } = render(dispatchData, { canDrawDiagrams: true });
    radio(el, 'clean').dispatchEvent(new Event('change'));
    await flush();
    expect(dispatchData).toHaveBeenLastCalledWith({
      type: 'characterUpdate',
      characterId: 'c1',
      character: { diagramStyle: 'clean' },
    });
    expect(lastToast()?.message).toBe('Their diagrams will be drawn clean and ruled');
    fixture.detectChanges();
    radio(el, 'handDrawn').dispatchEvent(new Event('change'));
    await flush();
    expect(dispatchData).toHaveBeenLastCalledWith({
      type: 'characterUpdate',
      characterId: 'c1',
      character: { diagramStyle: 'handDrawn' },
    });
    expect(lastToast()?.message).toBe('Their diagrams will be sketched by hand');
  });

  it('v5: the fieldset is disabled while unticked, and keeps its value', () => {
    const { el, fixture } = render(async () => ({}), { canDrawDiagrams: false, diagramStyle: 'clean' });
    expect(fieldset(el).disabled).toBe(true);
    expect(fieldset(el).getAttribute('aria-disabled')).toBe('true');
    expect(radio(el, 'clean').checked).toBe(true);
    fixture.componentRef.setInput('canDrawDiagrams', true);
    fixture.detectChanges();
    expect(fieldset(el).disabled).toBe(false);
    expect(fieldset(el).getAttribute('aria-disabled')).toBe('false');
    expect(radio(el, 'clean').checked).toBe(true);
  });

  it('v5: a rejected dispatch raises the error toast and the console line', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { el } = render(async () => {
      throw new Error('');
    });
    checkbox(el).dispatchEvent(new Event('change'));
    await flush();
    expect(lastToast()).toEqual({ type: 'error', message: 'Failed to update diagram settings' });
    expect(error).toHaveBeenCalledWith('Failed to save diagram settings', { error: '' });
  });

  it('v5: characterId null → everything disabled', () => {
    const { el, fixture } = render(async () => ({}), { canDrawDiagrams: true });
    fixture.componentRef.setInput('characterId', null);
    fixture.detectChanges();
    expect(checkbox(el).disabled).toBe(true);
    expect(fieldset(el).disabled).toBe(true);
  });

  it('v5: an archived (read-only) page disables the card', () => {
    const { el } = render(async () => ({}), { canDrawDiagrams: true, disabled: true });
    expect(checkbox(el).disabled).toBe(true);
    expect(fieldset(el).disabled).toBe(true);
  });
});
