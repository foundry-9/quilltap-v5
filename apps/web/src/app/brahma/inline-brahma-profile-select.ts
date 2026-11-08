import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ConnectionProfileDto } from '../core/core-contract';

/**
 * One connection profile as the select shows it — v4
 * `hooks/useConnectionProfiles.ts` `mapProfiles`'s coercions (`name || ''`,
 * `provider || ''`, `modelName || ''`, `isDefault === true`,
 * `allowToolUse !== false`, so an absent `allowToolUse` reads TRUE). v4's hook
 * is not ported as a module: this select is its only consumer here (P4.D388
 * Tier 3 item 32).
 */
export interface InlineBrahmaProfileOption {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isDefault: boolean;
  allowToolUse: boolean;
}

/** v4 `mapProfiles`, the five coercions the select reads. */
export function toInlineBrahmaProfileOption(p: ConnectionProfileDto): InlineBrahmaProfileOption {
  const raw = p as unknown as Record<string, unknown>;
  return {
    id: raw['id'] as string,
    name: (raw['name'] as string) || '',
    provider: (raw['provider'] as string) || '',
    modelName: (raw['modelName'] as string) || '',
    isDefault: raw['isDefault'] === true,
    allowToolUse: raw['allowToolUse'] !== false,
  };
}

/**
 * An option's text — v4 `InlineBrahmaProfileSelect.tsx`'s JSX, whose line
 * breaks between expressions JSX drops: `{name} ({provider}[ • {modelName}])[
 * — your default][ — no tools]`.
 */
export function inlineBrahmaOptionLabel(p: InlineBrahmaProfileOption): string {
  return (
    `${p.name} (${p.provider}${p.modelName ? ` • ${p.modelName}` : ''})` +
    `${p.isDefault ? ' — your default' : ''}` +
    `${!p.allowToolUse ? ' — no tools' : ''}`
  );
}

/**
 * InlineBrahmaProfileSelect
 *
 * A native `<select>` of the user's connection profiles for choosing which model
 * answers an inline `@Brahma:` / `@Brahma?` consultation. Used by the global
 * setting (Settings → Chat → Brahma Console) and the per-project override
 * (Prospero project page → Model Behavior). The empty value means "inherit" —
 * the caller names what is inherited via `inheritLabel`. A saved id that no
 * longer names a profile is shown as such (the server falls through past it).
 *
 * Callers supply the accessible name: a `<label for={inputId}>` or
 * `ariaLabelledBy`.
 *
 * v5 port of v4 `components/brahma-console/InlineBrahmaProfileSelect.tsx`
 * (`c539c82e0`, at the pin `624d0d25c`). Profiles come from the shared
 * `['connection-profiles']` query, cached as `ConnectionProfileDto[]` exactly as
 * every other reader of that key caches it.
 *
 * CONTROLLED, as v4's is (P4.D388 PR-2): a change emits the choice and then puts
 * the DOM back on the stored `value`, so the select shows what is saved until
 * the caller's `value` input moves (a successful save + refetch / cache write).
 * React does exactly that snap-back; it also makes a refused save self-heal.
 */
@Component({
  selector: 'qt-inline-brahma-profile-select',
  changeDetection: ChangeDetectionStrategy.OnPush,
  styles: [':host { display: block; }'],
  template: `
    <select
      [attr.id]="inputId() ?? null"
      [attr.aria-labelledby]="ariaLabelledBy() ?? null"
      [attr.class]="selectClass()"
      [disabled]="disabled() || loading()"
      (change)="onChange($any($event.target))"
    >
      <option value="" [selected]="selected() === ''">{{ inheritLabel() }}</option>
      @if (selectedMissing()) {
        <option [value]="selected()" [selected]="true">
          (a profile that has since left the building)
        </option>
      }
      @for (p of profiles(); track p.id) {
        <option [value]="p.id" [selected]="selected() === p.id">{{ label(p) }}</option>
      }
    </select>
  `,
})
export class InlineBrahmaProfileSelect {
  private readonly core = inject(CoreClient);

  readonly inputId = input<string | undefined>(undefined);
  readonly ariaLabelledBy = input<string | undefined>(undefined);
  readonly value = input<string | null | undefined>(undefined);
  readonly inheritLabel = input.required<string>();
  readonly disabled = input(false);
  readonly selectClass = input('qt-select w-full max-w-md');

  readonly profileChange = output<string | null>();

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: async (): Promise<ConnectionProfileDto[]> => {
      const resp = await this.core.dispatchExpect(
        { type: 'connectionProfileList' },
        'connectionProfiles',
      );
      return resp.data.profiles;
    },
  }));

  protected readonly loading = computed(() => this.profilesQuery.isLoading());
  protected readonly profiles = computed(() =>
    (this.profilesQuery.data() ?? []).map(toInlineBrahmaProfileOption),
  );
  protected readonly selected = computed(() => this.value() ?? '');
  protected readonly selectedMissing = computed(
    () =>
      !this.loading() &&
      this.selected() !== '' &&
      !this.profiles().some((p) => p.id === this.selected()),
  );

  protected label(p: InlineBrahmaProfileOption): string {
    return inlineBrahmaOptionLabel(p);
  }

  protected onChange(target: HTMLSelectElement): void {
    const chosen = target.value;
    this.profileChange.emit(chosen || null);
    // React's controlled snap-back: show the stored value until `value` moves.
    target.value = this.selected();
  }
}
