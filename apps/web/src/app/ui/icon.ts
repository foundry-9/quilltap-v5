import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/**
 * The theme-replaceable icon set (v4 `components/ui/icons/icon-registry.ts`).
 * Rendered as a CSS `mask-image` of `/images/icons/<name>.svg` tinted by
 * `currentColor` (except `brand`, a full-colour background image). The 85-name
 * registry is a public contract for theme overrides.
 */
export type IconName =
  | 'close' | 'pencil' | 'refresh' | 'check' | 'check-circle' | 'chat' | 'info'
  | 'trash' | 'copy' | 'plus' | 'minus' | 'search' | 'download' | 'upload'
  | 'cloud-upload' | 'external-link' | 'link' | 'send' | 'paperclip' | 'eye'
  | 'eye-off' | 'star' | 'bookmark' | 'tag' | 'expand' | 'compress'
  | 'chevron-down' | 'chevron-right' | 'chevron-left' | 'arrow-left'
  | 'arrow-right' | 'arrow-up' | 'arrow-down' | 'sort' | 'alert-triangle'
  | 'alert-circle' | 'shield' | 'ban' | 'clock' | 'calendar' | 'image'
  | 'camera' | 'play' | 'pause' | 'stop' | 'zoom-in' | 'zoom-out' | 'projects'
  | 'files' | 'file' | 'file-plus' | 'folder' | 'folder-plus' | 'book'
  | 'characters' | 'scriptorium' | 'photos' | 'scenarios' | 'profile' | 'user'
  | 'user-plus' | 'users' | 'megaphone' | 'mail' | 'dice' | 'sparkles' | 'wand'
  // The rocking quill shown while a reply is awaited or streaming. The MOTION
  // is a separate theme hook (`.qt-thinking-indicator` in _chat.css) so a theme
  // can change the glyph and the animation independently. Deliberately NOT
  // `brand` — a theme that swaps the brand mark for a wordmark should not find
  // that wordmark rocking in the Salon. Mask mode means it tints with
  // currentColor, so the call sites' qt-text-secondary and the status strip's
  // per-stage colors reach it. (v4 carries an `ariaLabel: 'Writing'` on the
  // registry entry; v5 has no per-name aria registry — `qt-quill-animation`
  // supplies the label at the call site.)
  | 'thinking'
  | 'wrench' | 'code' | 'cpu' | 'database' | 'layers' | 'zap' | 'swap'
  | 'log-out' | 'settings' | 'themes' | 'wardrobe' | 'help' | 'brahma-console'
  | 'sun' | 'moon' | 'monitor' | 'brand';

/**
 * `<qt-icon [name]="'chat'">` → `<span data-icon="chat" class="qt-icon">`.
 * Glyph + tint are pure CSS (the `_icons.css` mask rules). Decorative unless a
 * `title` is given (then `role="img"` + `aria-label`). Sizing MUST come from the
 * call site's classes (Tailwind `w-7 h-7`), never frozen here.
 */
@Component({
  selector: 'qt-icon',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<span
    [class]="cssClass()"
    [attr.data-icon]="name()"
    [attr.role]="title() ? 'img' : null"
    [attr.aria-label]="title() ?? null"
    [attr.aria-hidden]="title() ? null : true"
  ></span>`,
})
export class Icon {
  readonly name = input.required<IconName>();
  readonly title = input<string | undefined>(undefined);
  /** Extra classes (e.g. Tailwind `w-7 h-7`). */
  readonly klass = input<string>('', { alias: 'class' });

  protected readonly cssClass = computed(() => `qt-icon ${this.klass()}`.trim());
}
