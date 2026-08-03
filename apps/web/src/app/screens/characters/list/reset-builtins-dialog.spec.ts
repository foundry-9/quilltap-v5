import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ToastService } from '../../../ui/toast.service';
import { ResetBuiltinsDialog } from './reset-builtins-dialog';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

async function render(): Promise<ComponentFixture<ResetBuiltinsDialog>> {
  TestBed.configureTestingModule({ imports: [ResetBuiltinsDialog] });
  const fixture = TestBed.createComponent(ResetBuiltinsDialog);
  fixture.detectChanges();
  return fixture;
}

function resetButton(fixture: ComponentFixture<ResetBuiltinsDialog>): HTMLButtonElement {
  return [...fixture.nativeElement.querySelectorAll('button')].find(
    (b: HTMLButtonElement) => b.textContent?.trim() === 'Reset Built-ins',
  ) as HTMLButtonElement;
}

/**
 * v4 `AuroraView.tsx:313-335` — neither outcome has an inline surface, both are
 * toasts only.
 */
describe('ResetBuiltinsDialog toasts', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('toasts the v4 success sentence, emits done, and closes', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 })),
    );
    const fixture = await render();
    let done = 0;
    let closed = 0;
    fixture.componentInstance.done.subscribe(() => done++);
    fixture.componentInstance.close.subscribe(() => closed++);

    resetButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([
      { type: 'success', message: 'Built-in characters reset successfully.' },
    ]);
    expect(done).toBe(1);
    expect(closed).toBe(1);
    expect(fixture.nativeElement.textContent).not.toContain('reset successfully');
  });

  it('toasts the server error and stays open on failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ error: 'the workshop is locked' }), { status: 500 }),
      ),
    );
    const fixture = await render();
    let done = 0;
    fixture.componentInstance.done.subscribe(() => done++);

    resetButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'the workshop is locked' }]);
    expect(done).toBe(0);
  });
});
