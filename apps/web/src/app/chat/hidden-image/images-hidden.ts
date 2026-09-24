import { InjectionToken, inject, signal, type Provider, type Signal } from '@angular/core';

import { QuickHideService } from '../../quick-hide/quick-hide.service';

/**
 * The "images hidden" switch for a subtree — the v5 twin of v4's
 * `ImagesHiddenContext` (`components/quick-hide/images-hidden-context.tsx`,
 * `e3937d7aa`).
 *
 * v4 carries the quick-hide "Salon Images" switch down the Salon through a
 * React context whose default is `false`; only `SalonView` provides it, so
 * shared components such as `Avatar` keep painting images on every page but
 * the one the operator asked to veil. The Angular equivalent is an element
 * injector: {@link provideImagesHidden} sits on `SalonConversation`'s
 * `providers`, and everything rendered from its template — the transcript,
 * the chat sidebar, the composer, the dialogs — resolves this token there. Any
 * other page falls through to the root default (`false`, v4's context
 * default).
 *
 * Portals: v4's React context crosses portals by construction. v5's portaled
 * dialogs (the Scenario Builder and its save dialog) MOVE their host node to
 * `<body>` after rendering in place, so they keep the injector they were
 * created with; the Salon's Inform, Insert Announcement and In Their Own
 * Words dialogs are not portaled at all (they render in `SalonConversation`'s
 * own template). Measured at P4.D223 — no dialog of the Salon's is created
 * through `createComponent` with a foreign injector.
 */
export const IMAGES_HIDDEN = new InjectionToken<Signal<boolean>>('IMAGES_HIDDEN', {
  providedIn: 'root',
  factory: () => NOT_HIDDEN,
});

const NOT_HIDDEN: Signal<boolean> = signal(false).asReadonly();

/** The Salon's provider: the quick-hide service's global `hideSalonImages`. */
export function provideImagesHidden(): Provider {
  return {
    provide: IMAGES_HIDDEN,
    useFactory: () => inject(QuickHideService).hideSalonImages,
  };
}

/** v4 `useImagesHidden()` — true when the surrounding subtree withholds its images. */
export function injectImagesHidden(): Signal<boolean> {
  return inject(IMAGES_HIDDEN);
}
