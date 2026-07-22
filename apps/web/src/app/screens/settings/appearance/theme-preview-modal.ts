import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { type ThemeOption } from '../../../theme/theme.service';
import { Modal } from '../../../ui/modal';
import { getContrastingTextColor, getMutedTextColor } from './contrast';
import { generateScopedThemeCSS, type ThemeTokens } from './scoped-theme-css';

let previewInstanceCounter = 0;

/**
 * The theme preview modal (v4
 * `components/settings/appearance/components/ThemePreviewModal.tsx` +
 * `ThemePreviewPanel.tsx`): a banner painted in the theme's own background with
 * a contrast-computed foreground, a live element preview under a Light/Dark
 * toggle, and Apply.
 *
 * Where v4 fetches `GET /api/v1/themes/{id}?action=tokens`
 * (`useThemePreview.ts:94`), v5 reads the pack's committed
 * `public/themes/<id>/tokens.json` — the same four token sections, no server
 * involved. The installed-theme registry stays deferred with the rest of the
 * `.qtap-theme` surface (`appearance-tab.ts` class docstring).
 *
 * DEFERRED, loudly — v4 arms v5's bundled packs cannot feed:
 *  - The image gallery ("Bundled Images"). v4 derives it from manifest
 *    `previewImage` / `subsystems` entries; no v5 pack declares `previewImage`,
 *    and the derived-subsystem arm needs a `theme.json` reader this lane does
 *    not add. v4 renders its own empty state in that case anyway.
 *  - The icon sheet ("Overridden Icons"). Only `madmans-box` declares an icon
 *    map, and its `icons/` directory was never copied into `public/themes/` —
 *    P4.d17 added exactly one file to it (`brand.svg`, which backs the pack's
 *    `thinking` override), so every other `<img>` would still 404.
 *  - `cssOverrides` scoping — see `scoped-theme-css.ts`.
 *  - The DEFAULT theme. v4 previews it from a `DEFAULT_THEME_TOKENS` constant;
 *    v5's default is the base stylesheet, not a token bundle, so there is
 *    nothing to render. The appearance tab omits Preview on that card rather
 *    than inventing tokens.
 *  - Source/deprecation badges (v4 `contrast.ts:70-79` `getSourceBadge`).
 *    v5's `ThemeOption` carries no `source` or `deprecated` field, so every
 *    bundled pack would read "Bundle" by construction — no information.
 *
 * ⚠ DIVERGENCE NEEDING A HUMAN NOD — the dark-mode polarity. v4 forces a
 * theme without dark support to preview LIGHT
 * (`ThemePreviewModal.tsx:86`: `supportsDarkMode ? mode : 'light'`). v5's own
 * `ThemeService.applyToDom` (`:201-202`) does the OPPOSITE for the one pack
 * that sets `supportsDarkMode: false` — Madman's Box is forced DARK, i.e. in
 * v5 the flag reads "single-mode pack", not "light-only pack". This port
 * follows V5's rule so the preview shows what applying will actually produce;
 * transcribing v4's literal would preview that pack in a mode the app never
 * renders.
 */
@Component({
  selector: 'qt-theme-preview-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal [title]="theme().name" maxWidth="3xl" (close)="close.emit()">
      <!-- Banner: the theme's own background, with a contrast-computed foreground. -->
      <div
        class="rounded-lg px-5 py-4 mb-4"
        [style.background]="bannerBackground()"
        [style.color]="bannerForeground()"
      >
        <h3 class="text-lg font-semibold">{{ theme().name }}</h3>
        <p class="text-sm mt-1" [style.color]="bannerMuted()">{{ theme().description }}</p>
      </div>

      <div class="flex items-center justify-between gap-4 mb-3">
        @if (theme().supportsDarkMode) {
          <div class="inline-flex rounded-md border qt-border-default overflow-hidden" role="group" aria-label="Preview color mode">
            <button
              type="button"
              class="px-3 py-1.5 text-sm"
              [class.qt-bg-muted]="effectiveMode() === 'light'"
              [attr.aria-pressed]="effectiveMode() === 'light'"
              (click)="mode.set('light')"
            >
              Light
            </button>
            <button
              type="button"
              class="px-3 py-1.5 text-sm"
              [class.qt-bg-muted]="effectiveMode() === 'dark'"
              [attr.aria-pressed]="effectiveMode() === 'dark'"
              (click)="mode.set('dark')"
            >
              Dark
            </button>
          </div>
        } @else {
          <!-- A single-mode pack: v5 renders it in the one mode it supports. -->
          <p class="qt-text-xs qt-text-muted">
            This theme ships a single colour mode.
          </p>
        }
      </div>

      @if (loading()) {
        <div class="py-8 text-center qt-text-secondary">Loading theme preview...</div>
      } @else if (error()) {
        <div class="py-8 text-center">
          <p class="qt-text-secondary">Failed to load theme preview</p>
          <p class="qt-text-xs qt-text-muted mt-1">{{ error() }}</p>
        </div>
      } @else {
        <!-- The scoped CSS is injected into head imperatively (see
             syncStyleElement below) - an inline style element in an Angular
             template is hoisted to component styles at compile time and would
             never see the interpolated value. -->
        <!-- v4 pins height:auto so a theme setting height:100dvh on root can't
             blow the preview out (ThemePreviewPanel.tsx:187-191). -->
        <div
          [class]="scopeClass"
          class="rounded-lg border qt-border-default p-5 space-y-4"
          style="height: auto; min-height: auto; max-height: none;"
        >
          <h2 class="qt-heading-2">Welcome to Quilltap</h2>
          <p>
            This is a preview of how the theme will look. The colors, fonts, and styles shown here
            will be applied throughout the application.
          </p>
          <div class="flex flex-wrap gap-2">
            <button type="button" class="qt-button-primary">Primary</button>
            <button type="button" class="qt-button-secondary">Secondary</button>
            <button type="button" class="qt-button-ghost">Ghost</button>
          </div>
          <input class="qt-input w-full" placeholder="Text input placeholder..." readonly />
          <div class="flex flex-wrap gap-2">
            <span class="qt-badge-character">Character</span>
            <span class="qt-badge-chat">Chat</span>
            <span class="qt-badge-secondary">Success</span>
            <span class="qt-tag-badge qt-tag-badge-md">Tag</span>
          </div>
          <div class="qt-card p-4">
            <h4 class="font-semibold">Card Example</h4>
            <p class="qt-text-small">Cards use the card background and border colors.</p>
          </div>
        </div>
      }

      <div qt-modal-footer class="flex justify-end gap-2">
        @if (isActive()) {
          <span class="qt-button-secondary qt-button-sm" aria-disabled="true">Active</span>
        } @else {
          <button type="button" class="qt-button-primary" (click)="apply.emit()">Apply</button>
        }
      </div>
    </qt-modal>
  `,
})
export class ThemePreviewModal {
  readonly theme = input.required<ThemeOption>();
  readonly isActive = input(false);

  readonly close = output<void>();
  readonly apply = output<void>();

  private readonly destroyRef = inject(DestroyRef);
  private styleEl: HTMLStyleElement | null = null;

  /** v4 keys its panel by theme id + mode + instance so scopes never collide. */
  protected readonly scopeClass = `theme-preview-${(previewInstanceCounter += 1)}`;

  protected readonly mode = signal<'light' | 'dark'>('light');
  protected readonly tokens = signal<ThemeTokens | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  /**
   * The mode actually rendered. See the class docstring: for a pack with
   * `supportsDarkMode: false`, v5 forces the mode `ThemeService.applyToDom`
   * would force (`:201-202`) — DARK — so preview matches reality.
   */
  protected readonly effectiveMode = computed<'light' | 'dark'>(() =>
    this.theme().supportsDarkMode ? this.mode() : 'dark',
  );

  protected readonly bannerBackground = computed(
    () => this.tokens()?.colors?.[this.effectiveMode()]?.['background'] || '#1a1a1a',
  );
  protected readonly bannerForeground = computed(() =>
    getContrastingTextColor(this.bannerBackground()),
  );
  protected readonly bannerMuted = computed(() => getMutedTextColor(this.bannerBackground()));

  protected readonly scopedCss = computed(() => {
    const tokens = this.tokens();
    if (!tokens) return '';
    return generateScopedThemeCSS(tokens, this.scopeClass, this.effectiveMode());
  });

  constructor() {
    effect(() => {
      const id = this.theme().id;
      void this.load(id);
    });
    // Keep the injected <style> in step with the computed CSS.
    effect(() => this.syncStyleElement(this.scopedCss()));
    this.destroyRef.onDestroy(() => {
      this.styleEl?.remove();
      this.styleEl = null;
    });
  }

  /**
   * v4 injects the scoped CSS as a `<style>` in the panel
   * (`ThemePreviewPanel.tsx:184`). Angular hoists template `<style>` blocks at
   * compile time, so v5 owns one head-level element per modal instance instead
   * — scoped by the unique class either way, so the live theme is untouched.
   */
  private syncStyleElement(css: string): void {
    if (typeof document === 'undefined') return;
    if (!css) {
      this.styleEl?.remove();
      this.styleEl = null;
      return;
    }
    if (!this.styleEl) {
      this.styleEl = document.createElement('style');
      this.styleEl.setAttribute('data-qt-theme-preview', this.scopeClass);
      document.head.appendChild(this.styleEl);
    }
    this.styleEl.textContent = css;
  }

  private async load(id: string | null): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    if (!id) {
      // The default theme has no token bundle — see the class docstring.
      this.error.set('The built-in theme has no bundled tokens to preview.');
      this.loading.set(false);
      return;
    }
    try {
      const res = await fetch(`/themes/${id}/tokens.json`);
      if (!res.ok) throw new Error(`tokens.json ${res.status}`);
      this.tokens.set((await res.json()) as ThemeTokens);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : String(err));
    } finally {
      this.loading.set(false);
    }
  }

  /** Exposed so the host can seed the toggle from the live mode. */
  setInitialMode(mode: 'light' | 'dark'): void {
    this.mode.set(mode);
  }
}
