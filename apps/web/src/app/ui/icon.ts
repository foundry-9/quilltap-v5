import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/**
 * The theme-replaceable icon set (v4 `components/ui/icons/icon-registry.ts`).
 * Rendered as a CSS `mask-image` of `/images/icons/<name>.svg` tinted by
 * `currentColor` (except `brand`, a full-colour background image). The 84-name
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
