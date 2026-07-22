import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { ProviderIcon } from '../../images/provider-icon';

/**
 * Provider glyph + model name (v4 `components/ui/ProviderModelBadge.tsx`),
 * rendered beside an LLM-controlled participant's name in the chat sidebar.
 * Renders nothing when `provider` is falsy — v4 returns null there (old rows,
 * user-controlled seats).
 *
 * **Reduced, loudly:** v4 threads `useProviders().getProviderIcon(provider)`
 * into `<ProviderIcon iconData=…>` so a plugin can supply a custom SVG. v5's
 * `qt-provider-icon` ports only the DEFAULT (abbreviation-circle) path — the
 * plugin icon registry is unported (that deferral is recorded on
 * `images/provider-icon.ts`), so this badge simply never has icon data to pass.
 * Everything else — the size table, the opacity, the `provider: model` title,
 * the truncation widths — is v4's.
 */
@Component({
  selector: 'qt-provider-model-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ProviderIcon],
  template: `
    @if (provider(); as p) {
      <span
        [class]="'inline-flex items-center gap-1 opacity-60 ' + textClass()"
        [title]="badgeTitle()"
      >
        <qt-provider-icon [provider]="p" [sizeClass]="iconClass()" />
        @if (modelName(); as model) {
          <span [class]="'truncate ' + widthClass()">{{ model }}</span>
        }
      </span>
    }
  `,
})
export class ProviderModelBadge {
  readonly provider = input<string | null | undefined>(undefined);
  readonly modelName = input<string | null | undefined>(undefined);
  /** 'xs' under chat avatars, 'sm' in the sidebar/cards (v4 `sizeConfig`). */
  readonly size = input<'xs' | 'sm'>('xs');
  /** Tooltip override; defaults to `provider: modelName` (v4). */
  readonly titleOverride = input<string | undefined>(undefined);
  /** Tighter max-width for narrow containers like cards (v4 `compact`). */
  readonly compact = input(false);

  protected readonly iconClass = computed(() => (this.size() === 'sm' ? 'h-3.5 w-3.5' : 'h-3 w-3'));
  protected readonly textClass = computed(() =>
    this.size() === 'sm' ? 'text-xs' : 'text-[10px]',
  );
  protected readonly widthClass = computed(() =>
    this.compact() ? 'max-w-[6rem]' : 'max-w-[8rem]',
  );
  protected readonly badgeTitle = computed(
    () => this.titleOverride() ?? `${this.provider()}: ${this.modelName() || 'unknown'}`,
  );
}
