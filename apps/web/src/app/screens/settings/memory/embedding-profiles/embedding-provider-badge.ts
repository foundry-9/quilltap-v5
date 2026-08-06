import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { PROVIDER_BADGE_CLASSES } from './embedding-profiles.types';

/**
 * The provider badge (v4 `components/settings/embedding-profiles/ProviderBadge.tsx`):
 * a `qt-badge-provider-*` span, falling back to `qt-badge-secondary` for an
 * unknown provider.
 */
@Component({
  selector: 'qt-embedding-provider-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<span [class]="badgeClass()">{{ provider() }}</span>`,
})
export class EmbeddingProviderBadge {
  readonly provider = input.required<string>();
  protected readonly badgeClass = computed(
    () => PROVIDER_BADGE_CLASSES[this.provider()] ?? 'qt-badge-secondary',
  );
}
