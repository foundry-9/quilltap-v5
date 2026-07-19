import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { applyImageNavigation, type ImageNavigationOptions } from './image-navigation';

/**
 * v4 `__tests__/unit/hooks/useImageNavigation.test.ts` (350 lines) ported
 * case-for-case. Each `renderHook` becomes one `applyImageNavigation` call;
 * each `rerender` disposes the previous application and applies the new
 * options (React's dep-change effect re-run); `unmount` is the final dispose.
 */
describe('applyImageNavigation (v4 useImageNavigation)', () => {
  let originalOverflow: string;
  let disposers: Array<() => void>;

  function apply(options: ImageNavigationOptions): () => void {
    const dispose = applyImageNavigation(options);
    disposers.push(dispose);
    return dispose;
  }

  function press(key: string): void {
    document.dispatchEvent(new KeyboardEvent('keydown', { key }));
  }

  beforeEach(() => {
    originalOverflow = document.body.style.overflow;
    disposers = [];
  });

  afterEach(() => {
    disposers.forEach((d) => d());
    document.body.style.overflow = originalOverflow;
    vi.clearAllMocks();
  });

  describe('escape key handling', () => {
    // v4 :23 — should call onClose when Escape is pressed and modal is open.
    it('calls onClose when Escape is pressed and the modal is open', () => {
      const onClose = vi.fn();
      apply({ isOpen: true, onClose });
      press('Escape');
      expect(onClose).toHaveBeenCalledTimes(1);
    });

    // v4 :39 — should not call onClose when modal is closed.
    it('does not call onClose when the modal is closed', () => {
      const onClose = vi.fn();
      apply({ isOpen: false, onClose });
      press('Escape');
      expect(onClose).not.toHaveBeenCalled();
    });

    // v4 :55 — should not handle Escape when handleEscape is false (the
    // nested-modal suppression PhotoGalleryModal.tsx:170-174 relies on).
    it('does not handle Escape when handleEscape is false', () => {
      const onClose = vi.fn();
      apply({ isOpen: true, onClose, handleEscape: false });
      press('Escape');
      expect(onClose).not.toHaveBeenCalled();
    });

    // v4 :72 — should handle Escape when handleEscape is explicitly true.
    it('handles Escape when handleEscape is explicitly true', () => {
      const onClose = vi.fn();
      apply({ isOpen: true, onClose, handleEscape: true });
      press('Escape');
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  describe('arrow key navigation', () => {
    // v4 :91 — should call onPrev when ArrowLeft is pressed.
    it('calls onPrev when ArrowLeft is pressed', () => {
      const onPrev = vi.fn();
      const onClose = vi.fn();
      apply({ isOpen: true, onClose, onPrev });
      press('ArrowLeft');
      expect(onPrev).toHaveBeenCalledTimes(1);
      expect(onClose).not.toHaveBeenCalled();
    });

    // v4 :110 — should call onNext when ArrowRight is pressed.
    it('calls onNext when ArrowRight is pressed', () => {
      const onNext = vi.fn();
      const onClose = vi.fn();
      apply({ isOpen: true, onClose, onNext });
      press('ArrowRight');
      expect(onNext).toHaveBeenCalledTimes(1);
      expect(onClose).not.toHaveBeenCalled();
    });

    // v4 :129 — should not call onPrev when not provided.
    it('ignores ArrowLeft when onPrev is not provided', () => {
      const onClose = vi.fn();
      apply({ isOpen: true, onClose });
      press('ArrowLeft');
      expect(onClose).not.toHaveBeenCalled();
    });

    // v4 :146 — should not call onNext when not provided.
    it('ignores ArrowRight when onNext is not provided', () => {
      const onClose = vi.fn();
      apply({ isOpen: true, onClose });
      press('ArrowRight');
      expect(onClose).not.toHaveBeenCalled();
    });

    // v4 :163 — should handle both navigation and escape.
    it('handles both navigation and escape', () => {
      const onClose = vi.fn();
      const onPrev = vi.fn();
      const onNext = vi.fn();
      apply({ isOpen: true, onClose, onPrev, onNext });
      press('ArrowLeft');
      expect(onPrev).toHaveBeenCalledTimes(1);
      press('ArrowRight');
      expect(onNext).toHaveBeenCalledTimes(1);
      press('Escape');
      expect(onClose).toHaveBeenCalledTimes(1);
    });

    // v4 :191 — should not handle arrow keys when modal is closed.
    it('ignores arrow keys when the modal is closed', () => {
      const onPrev = vi.fn();
      const onNext = vi.fn();
      const onClose = vi.fn();
      apply({ isOpen: false, onClose, onPrev, onNext });
      press('ArrowLeft');
      press('ArrowRight');
      expect(onPrev).not.toHaveBeenCalled();
      expect(onNext).not.toHaveBeenCalled();
    });
  });

  describe('body scroll prevention', () => {
    // v4 :217 — should prevent body scroll when modal is open.
    it('prevents body scroll when the modal is open', () => {
      apply({ isOpen: true, onClose: vi.fn() });
      expect(document.body.style.overflow).toBe('hidden');
    });

    // v4 :228 — should restore body scroll when modal closes (rerender).
    it('restores body scroll when the modal closes', () => {
      const dispose = applyImageNavigation({ isOpen: true, onClose: vi.fn() });
      expect(document.body.style.overflow).toBe('hidden');
      dispose(); // the rerender's cleanup
      apply({ isOpen: false, onClose: vi.fn() });
      expect(document.body.style.overflow).toBe('');
    });

    // v4 :245 — should restore body scroll on unmount.
    it('restores body scroll on dispose', () => {
      const dispose = applyImageNavigation({ isOpen: true, onClose: vi.fn() });
      expect(document.body.style.overflow).toBe('hidden');
      dispose();
      expect(document.body.style.overflow).toBe('');
    });

    // v4 :260 — should not prevent scroll when preventBodyScroll is false.
    it('does not prevent scroll when preventBodyScroll is false', () => {
      apply({ isOpen: true, onClose: vi.fn(), preventBodyScroll: false });
      expect(document.body.style.overflow).toBe('');
    });

    // v4 :272 — should handle preventBodyScroll option correctly (rerender
    // true → false restores).
    it('handles a preventBodyScroll change across re-applications', () => {
      const dispose = applyImageNavigation({
        isOpen: true,
        onClose: vi.fn(),
        preventBodyScroll: true,
      });
      expect(document.body.style.overflow).toBe('hidden');
      dispose();
      apply({ isOpen: true, onClose: vi.fn(), preventBodyScroll: false });
      expect(document.body.style.overflow).toBe('');
    });
  });

  describe('cleanup', () => {
    // v4 :292 — should remove event listeners on unmount.
    it('removes the keydown listener on dispose', () => {
      const onClose = vi.fn();
      const onPrev = vi.fn();
      const onNext = vi.fn();
      const dispose = applyImageNavigation({ isOpen: true, onClose, onPrev, onNext });
      dispose();
      press('Escape');
      press('ArrowLeft');
      press('ArrowRight');
      expect(onClose).not.toHaveBeenCalled();
      expect(onPrev).not.toHaveBeenCalled();
      expect(onNext).not.toHaveBeenCalled();
    });
  });

  describe('callback updates', () => {
    // v4 :324 — should use updated callbacks.
    it('uses updated callbacks after a re-application', () => {
      const onClose1 = vi.fn();
      const onClose2 = vi.fn();
      const dispose = applyImageNavigation({ isOpen: true, onClose: onClose1 });
      press('Escape');
      expect(onClose1).toHaveBeenCalledTimes(1);
      expect(onClose2).not.toHaveBeenCalled();
      dispose();
      apply({ isOpen: true, onClose: onClose2 });
      press('Escape');
      expect(onClose1).toHaveBeenCalledTimes(1);
      expect(onClose2).toHaveBeenCalledTimes(1);
    });
  });
});
