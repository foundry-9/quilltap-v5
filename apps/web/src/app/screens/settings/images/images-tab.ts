import { ChangeDetectionStrategy, Component, OnInit, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ImageProfilesCard } from './image-profiles-card';

/**
 * The Images tab (v4 `components/settings/tabs/ImagesTabContent.tsx`): the Image
 * Profiles collapsible card. The card seeds `defaultOpen` in `ngOnInit`
 * ([[p4.6l-groups-projects-spa]] gotcha) and honours the `?section=` deep link.
 */
@Component({
  selector: 'qt-settings-images',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ImageProfilesCard],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        The Lantern — where the engines that conjure pictures from prose are configured.
      </p>

      <qt-collapsible-card
        title="Image Profiles"
        description="Configure image generation providers and models"
        sectionId="image-profiles"
        [defaultOpen]="defaultOpen()"
        [forceOpen]="section() === 'image-profiles'"
      >
        <qt-image-profiles-card />
      </qt-collapsible-card>
    </div>
  `,
})
export class ImagesTab implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly queryParams = toSignal(this.route.queryParamMap, { requireSync: true });

  protected readonly section = computed(() => this.queryParams().get('section'));
  protected readonly defaultOpen = signal(false);

  ngOnInit(): void {
    this.defaultOpen.set(true);
  }
}
