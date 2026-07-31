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
  // ⚠ Load-bearing, do not remove. `class` is an INPUT here (aliased below) that
  // lands on the inner span — but Angular ALSO keeps the same static `class` on
  // this host element, so a site writing `class="absolute left-3 …"` gets the
  // offset applied TWICE: the host becomes a positioned box at left:12px, and
  // the span then offsets another 12px inside it. v4's React `<Icon>` renders
  // ONE span, so v5 was landing every positioned icon at double its offset —
  // measured at the toolbar search bar, where the glyph sat 4px PAST the start
  // of the input text (v4 clears it by 8px; dogfood finding #44).
  //
  // `display: contents` makes the host generate no box at all, so it cannot be a
  // containing block and the host's copy of `absolute`/`left-*` has nothing to
  // act on — the span resolves against the real positioned ancestor, exactly as
  // v4's single element does. Verified in the browser rather than reasoned from
  // the spec (blockification does NOT apply here), and pinned by a measured beat
  // in `page-toolbar-flow.spec.ts`.
  host: { style: 'display: contents' },
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
