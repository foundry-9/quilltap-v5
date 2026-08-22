import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/**
 * Provider defaults — v4 `components/image-profiles/ProviderIcon.tsx`
 * `PROVIDER_DEFAULTS` (`:52-63`), transcribed name-for-name. An unknown
 * provider falls back to the secondary colour and its first three characters
 * upper-cased (v4 `:170-173`).
 */
const PROVIDER_DEFAULTS: Record<string, { color: string; abbrev: string }> = {
  OPENAI: { color: 'qt-text-success', abbrev: 'OAI' },
  GROK: { color: 'qt-text-primary', abbrev: 'XAI' },
  GOOGLE_IMAGEN: { color: 'qt-text-info', abbrev: 'GGL' },
  GOOGLE: { color: 'qt-text-info', abbrev: 'GGL' },
  OPENROUTER: { color: 'qt-text-warning', abbrev: 'OR' },
  Z_AI: { color: 'qt-text-success', abbrev: 'ZAI' },
  NANOGPT: { color: 'qt-text-primary', abbrev: 'NGPT' },
  ETERNAL_AI: { color: 'qt-text-primary', abbrev: 'EAI' },
  ANTHROPIC: { color: 'qt-text-warning', abbrev: 'ANT' },
  OLLAMA: { color: 'qt-text-secondary', abbrev: 'OLL' },
  OPENAI_COMPATIBLE: { color: 'qt-text-secondary', abbrev: 'OAC' },
};

/** v4 `ProviderIcon` defaults for one provider (exported for the spec). */
export function providerIconDefaults(provider: string): { color: string; abbrev: string } {
  return (
    PROVIDER_DEFAULTS[provider] ?? {
      color: 'qt-text-secondary',
      abbrev: provider.slice(0, 3).toUpperCase(),
    }
  );
}

/**
 * The provider glyph — a port of v4
 * `components/image-profiles/ProviderIcon.tsx`'s **DefaultIcon** path
 * (`:135-163`): a filled circle carrying the provider's abbreviation, tinted by
 * the provider's colour class. The abbreviation drops to font-size 9 when it
 * runs longer than three characters (v4 `:155`).
 *
 * **Deferred (loud, named):** v4's plugin `iconData` branch (`CustomIcon`,
 * `:66-133` — raw-SVG and structured-SVG rendering), the `abbreviation` /
 * `colorClass` prop overrides, and the sibling `ProviderBadge` export
 * (`:196-224`). None of them are reachable from this lane's only call site —
 * v4's `ImageProfilePicker` detail card renders a bare
 * `<ProviderIcon provider={selected.provider} />` (`ImageProfilePicker.tsx:119`)
 * with no icon data and no overrides. The plugin-icon surface belongs to
 * whichever lane ports v4's plugin icon registry; it is NOT stubbed here.
 *
 * **Carried for that lane (P4.D102):** `ProviderBadge`'s `PROVIDER_BADGES`
 * table grew two rows this round that have no v5 home yet, so they are recorded
 * here rather than silently dropped — v4 `:220-221`:
 * `Z_AI: {bg: 'qt-bg-success/10', text: 'qt-text-success', label: 'Z.AI'}` and
 * `NANOGPT: {bg: 'qt-bg-primary/10', text: 'qt-text-primary', label: 'NanoGPT'}`.
 * Whoever lands `ProviderBadge` transcribes them verbatim.
 */
@Component({
  selector: 'qt-provider-icon',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <svg
      [class]="colorClass() + ' ' + sizeClass()"
      fill="currentColor"
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      [attr.aria-label]="provider()"
      role="img"
    >
      <circle cx="12" cy="12" r="12" />
      <text
        x="50%"
        y="50%"
        text-anchor="middle"
        dominant-baseline="middle"
        fill="white"
        [attr.font-size]="fontSize()"
        font-weight="bold"
        font-family="system-ui, -apple-system, sans-serif"
      >
        {{ abbrev() }}
      </text>
    </svg>
  `,
})
export class ProviderIcon {
  readonly provider = input.required<string>();
  /** v4's `className` default is `h-5 w-5` (`ProviderIcon.tsx:166`). */
  readonly sizeClass = input('h-5 w-5');

  private readonly defaults = computed(() => providerIconDefaults(this.provider()));
  protected readonly colorClass = computed(() => this.defaults().color);
  protected readonly abbrev = computed(() => this.defaults().abbrev);
  /** v4 `:155`: `abbreviation.length > 3 ? '9' : '10'`. */
  protected readonly fontSize = computed(() => (this.abbrev().length > 3 ? '9' : '10'));
}
