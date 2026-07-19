/**
 * Keyboard navigation for image modals/viewers — the port of v4
 * `hooks/useImageNavigation.ts` (81 lines).
 *
 * Handles (v4 `:26-32`):
 * - Escape key to close (suppressible via `handleEscape` — v4 `:14-18`: "Set
 *   to false if you want to conditionally handle Escape (e.g., in nested
 *   modals)". PhotoGalleryModal passes `handleEscape: selectedIndex === -1`
 *   so one Escape press never closes both layers.)
 * - ArrowLeft for previous image / ArrowRight for next (only when the
 *   callback is provided — the conditional-`undefined` arrow idiom)
 * - Prevents body scroll while open (`preventBodyScroll`, default true)
 *
 * The React hook re-runs its effect whenever `isOpen`, the callbacks, or the
 * options change (`useImageNavigation.ts:65-80`); the Angular equivalent is:
 * call {@link applyImageNavigation} from an `effect()` that reads the same
 * signals, dispose the previous application first (the effect's cleanup), and
 * the listener/body-scroll lifecycle is identical.
 *
 * @module images/image-navigation
 */

export interface ImageNavigationOptions {
  /** Whether the modal/viewer is currently open. */
  isOpen: boolean;
  /** Callback to close the modal. */
  onClose: () => void;
  /** Callback for previous image (optional — if provided, ArrowLeft triggers it). */
  onPrev?: () => void;
  /** Callback for next image (optional — if provided, ArrowRight triggers it). */
  onNext?: () => void;
  /**
   * Whether to handle Escape key. Default: true.
   * Set to false to conditionally handle Escape (e.g., in nested modals).
   */
  handleEscape?: boolean;
  /** Whether to prevent body scroll when open. Default: true. */
  preventBodyScroll?: boolean;
}

/**
 * Apply one image-navigation "effect run" (v4 `useImageNavigation.ts:65-80`):
 * when open, install the keydown listener and (optionally) lock body scroll;
 * the returned disposer removes the listener and restores scroll — exactly the
 * hook's cleanup. Callers re-apply on every option change, disposing the
 * previous run first (React's dep-change semantics).
 */
export function applyImageNavigation(options: ImageNavigationOptions): () => void {
  const { isOpen, onClose, onPrev, onNext, handleEscape = true, preventBodyScroll = true } = options;

  // v4 `:52-63` — the handler reads the CURRENT options (a fresh closure per
  // effect run, which is how "callback updates" behave in the hook).
  const handleKeyDown = (e: KeyboardEvent): void => {
    if (e.key === 'Escape' && handleEscape) {
      onClose();
    } else if (e.key === 'ArrowLeft' && onPrev) {
      onPrev();
    } else if (e.key === 'ArrowRight' && onNext) {
      onNext();
    }
  };

  if (isOpen) {
    document.addEventListener('keydown', handleKeyDown);
    if (preventBodyScroll) {
      document.body.style.overflow = 'hidden';
    }
  }

  return () => {
    document.removeEventListener('keydown', handleKeyDown);
    if (preventBodyScroll) {
      document.body.style.overflow = '';
    }
  };
}
