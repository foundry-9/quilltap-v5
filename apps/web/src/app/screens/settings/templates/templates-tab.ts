import { ChangeDetectionStrategy, Component, OnInit, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { RoleplayTemplatesCard } from './roleplay-templates-card';

/**
 * The Templates & Prompts tab (v4 `components/settings/tabs/
 * TemplatesPromptsTabContent.tsx`): the Roleplay Templates collapsible card.
 * v4's Prompts and "Aurora's Core Whisper" cards are named deferrals (their
 * verticals land later). The card seeds `defaultOpen` in `ngOnInit`
 * ([[p4.6l-groups-projects-spa]] gotcha) and honours the `?section=` deep link.
 */
@Component({
  selector: 'qt-settings-templates',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, RoleplayTemplatesCard],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        Aurora's workshop — where the house styles of narration and dialogue are drafted and kept.
      </p>

      <qt-collapsible-card
        title="Roleplay Templates"
        description="Manage templates that shape how characters interact"
        sectionId="roleplay-templates"
        [defaultOpen]="defaultOpen()"
        [forceOpen]="section() === 'roleplay-templates'"
      >
        <qt-roleplay-templates-card />
      </qt-collapsible-card>
    </div>
  `,
})
export class TemplatesTab implements OnInit {
  // Optional so the tab renders when hosted as a workspace tab (no ActivatedRoute);
  // v4 ignores `?section=` when hosted, so the deep-link simply falls back to null.
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;

  protected readonly section = computed(() => this.queryParams?.().get('section') ?? null);
  protected readonly defaultOpen = signal(false);

  ngOnInit(): void {
    // Open the sole card by default (bound inputs have resolved by ngOnInit).
    this.defaultOpen.set(true);
  }
}
