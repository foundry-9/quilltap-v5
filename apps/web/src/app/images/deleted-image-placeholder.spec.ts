import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { DeletedImagePlaceholder } from './deleted-image-placeholder';
import { ToastService } from '../ui/toast.service';

/**
 * v4 `__tests__/unit/deleted-image-placeholder.test.tsx` (272 lines) ported
 * case-for-case. v4 mocks `showConfirmation`/`showErrorToast`; v5's component
 * uses `window.confirm` and an INLINE error line (the no-toast-service
 * divergence), so those assertions target the confirm spy and the rendered
 * error text.
 */
/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('DeletedImagePlaceholder', () => {
  const mockImageId = 'test-image-id';
  const mockFilename = 'test-image.png';

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  async function render(inputs?: {
    width?: number;
    height?: number;
    styleClass?: string;
    remove?: () => Promise<void>;
  }): Promise<{
    fixture: ComponentFixture<DeletedImagePlaceholder>;
    cleanups: number[];
  }> {
    TestBed.configureTestingModule({ imports: [DeletedImagePlaceholder] });
    const fixture = TestBed.createComponent(DeletedImagePlaceholder);
    fixture.componentRef.setInput('imageId', mockImageId);
    fixture.componentRef.setInput('filename', mockFilename);
    if (inputs?.width !== undefined) fixture.componentRef.setInput('width', inputs.width);
    if (inputs?.height !== undefined) fixture.componentRef.setInput('height', inputs.height);
    if (inputs?.styleClass !== undefined) {
      fixture.componentRef.setInput('styleClass', inputs.styleClass);
    }
    if (inputs?.remove !== undefined) fixture.componentRef.setInput('remove', inputs.remove);
    const cleanups: number[] = [];
    fixture.componentInstance.cleanup.subscribe(() => cleanups.push(1));
    fixture.detectChanges();
    return { fixture, cleanups };
  }

  function removeButton(fixture: ComponentFixture<DeletedImagePlaceholder>): HTMLButtonElement {
    return Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      /remove/i.test((b as HTMLButtonElement).textContent ?? ''),
    ) as HTMLButtonElement;
  }

  async function flush(fixture: ComponentFixture<DeletedImagePlaceholder>): Promise<void> {
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  describe('Rendering', () => {
    // v4 :32 — default (non-compact) styling.
    it('renders with default (non-compact) styling', async () => {
      const { fixture } = await render();
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Image Deleted');
      expect(text).toContain(mockFilename);
      expect(removeButton(fixture)).toBeTruthy();
    });

    // v4 :46 — compact styling when !p-2 class is present (filename hidden).
    it('renders compact when the class string carries !p-2', async () => {
      const { fixture } = await render({ styleClass: '!p-2' });
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Image Deleted');
      expect(text).not.toContain(mockFilename);
      expect(removeButton(fixture)).toBeTruthy();
    });

    // v4 :62 — custom width/height applied.
    it('applies custom width and height when provided', async () => {
      const { fixture } = await render({ width: 500, height: 300 });
      const element = fixture.nativeElement.querySelector('div') as HTMLElement;
      expect(element.style.width).toBe('500px');
      expect(element.style.height).toBe('300px');
    });

    // v4 :78 — no width/height when w-full/h-full classes are present.
    it('skips width/height when the class string carries w-full h-full', async () => {
      const { fixture } = await render({ width: 500, height: 300, styleClass: 'w-full h-full' });
      const element = fixture.nativeElement.querySelector('div') as HTMLElement;
      expect(element.style.width).toBe('');
      expect(element.style.height).toBe('');
    });

    // v4 :95 — custom className lands on the element.
    it('renders with a custom class', async () => {
      const { fixture } = await render({ styleClass: 'custom-class' });
      const element = fixture.nativeElement.querySelector('div') as HTMLElement;
      expect(element.classList.contains('custom-class')).toBe(true);
    });
  });

  describe('Cleanup functionality', () => {
    // v4 :110 — onCleanup after successful deletion (confirm copy verbatim).
    it('emits cleanup after a successful deletion', async () => {
      const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
      const fetchMock = vi.fn(async () => ({ ok: true, json: async () => ({ success: true }) }));
      vi.stubGlobal('fetch', fetchMock);

      const { fixture, cleanups } = await render();
      removeButton(fixture).click();
      await flush(fixture);

      expect(confirm).toHaveBeenCalledWith(
        'This image file has been deleted. Remove this reference from the database?',
      );
      expect(fetchMock).toHaveBeenCalledWith(`/api/v1/images/${mockImageId}`, {
        method: 'DELETE',
      });
      expect(cleanups.length).toBe(1);
    });

    // v4 :146 — cancel stops everything.
    it('does not proceed when the user cancels the confirmation', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(false);
      const fetchMock = vi.fn();
      vi.stubGlobal('fetch', fetchMock);

      const { fixture, cleanups } = await render();
      removeButton(fixture).click();
      await flush(fixture);

      expect(fetchMock).not.toHaveBeenCalled();
      expect(cleanups.length).toBe(0);
    });

    // v4 :169 — the server error message surfaces (toast → inline line).
    it('shows the server error message on deletion failure', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      vi.stubGlobal(
        'fetch',
        vi.fn(async () => ({ ok: false, json: async () => ({ error: 'Failed to delete image' }) })),
      );

      const { fixture, cleanups } = await render();
      removeButton(fixture).click();
      await flush(fixture);

      expect(toasts()).toEqual([{ type: 'error', message: 'Failed to delete image' }]);
      expect(cleanups.length).toBe(0);
    });

    // v4 :196 — a network error is handled gracefully.
    it('handles network errors gracefully', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      vi.stubGlobal(
        'fetch',
        vi.fn(async () => {
          throw new Error('Network error');
        }),
      );

      const { fixture, cleanups } = await render();
      removeButton(fixture).click();
      await flush(fixture);

      expect(toasts()).toEqual([{ type: 'error', message: 'Network error' }]);
      expect(cleanups.length).toBe(0);
    });

    // v4 :220 — works without an onCleanup callback (nothing subscribes).
    it('works without a cleanup subscriber', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      const fetchMock = vi.fn(async () => ({ ok: true, json: async () => ({ success: true }) }));
      vi.stubGlobal('fetch', fetchMock);

      TestBed.configureTestingModule({ imports: [DeletedImagePlaceholder] });
      const fixture = TestBed.createComponent(DeletedImagePlaceholder);
      fixture.componentRef.setInput('imageId', mockImageId);
      fixture.componentRef.setInput('filename', mockFilename);
      fixture.detectChanges();

      removeButton(fixture).click();
      await flush(fixture);
      expect(fetchMock).toHaveBeenCalled();
    });
  });

  /**
   * P4.D286 — bug 194 (v4 `9ecebf986` `DeletedImagePlaceholder.tsx:11-17`,
   * `:41-55`): an `onRemove` override removes the reference through the
   * record's own route INSTEAD of the generic DELETE (which in v5 is a loud
   * refusal — the override is the only working Remove); a throw toasts.
   */
  describe('The remove override (bug 194)', () => {
    it('calls the override instead of the generic DELETE, then emits cleanup', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      const fetchMock = vi.fn();
      vi.stubGlobal('fetch', fetchMock);
      const remove = vi.fn(async () => undefined);
      const { fixture, cleanups } = await render({ remove });
      removeButton(fixture).click();
      await flush(fixture);
      expect(remove).toHaveBeenCalledTimes(1);
      expect(fetchMock).not.toHaveBeenCalled();
      expect(cleanups.length).toBe(1);
    });

    it('does not call it when the user cancels', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(false);
      const remove = vi.fn(async () => undefined);
      const { fixture, cleanups } = await render({ remove });
      removeButton(fixture).click();
      await flush(fixture);
      expect(remove).not.toHaveBeenCalled();
      expect(cleanups.length).toBe(0);
    });

    it('toasts a thrown message and emits no cleanup', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      const { fixture, cleanups } = await render({
        remove: async () => {
          throw new Error('That picture is not one of this item’s');
        },
      });
      removeButton(fixture).click();
      await flush(fixture);
      expect(toasts()).toEqual([{ type: 'error', message: 'That picture is not one of this item’s' }]);
      expect(cleanups.length).toBe(0);
    });
  });

  describe('Accessibility', () => {
    // v4 :247 — proper button role.
    it('exposes the Remove control as a button', async () => {
      const { fixture } = await render();
      expect(fixture.nativeElement.querySelector('button')).toBeTruthy();
    });

    // v4 :260 — descriptive text for screen readers.
    it('carries the descriptive Image Deleted text', async () => {
      const { fixture } = await render();
      expect(fixture.nativeElement.textContent).toContain('Image Deleted');
    });
  });
});
