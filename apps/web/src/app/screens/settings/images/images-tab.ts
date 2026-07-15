import { ChangeDetectionStrategy, Component, OnInit, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ImageProfilesCard } from './image-profiles-card';
import { StoryBackgroundsCard } from './story-backgrounds-card';

/**
 * The Images tab (v4 `components/settings/tabs/ImagesTabContent.tsx`): the Image
 * Profiles and Story Backgrounds collapsible cards, in v4's order. The cards
 * seed `defaultOpen` in `ngOnInit` ([[p4.6l-groups-projects-spa]] gotcha) and
 * honour the `?section=` deep link.
 *
 * Still deferred here: v4's third card, **Default Aesthetics** (the two
 * `AestheticEditorField`s over `/api/v1/system/image-aesthetics`) — its own
 * surface, not this order's.
 */
@Component({
  selector: 'qt-settings-images',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ImageProfilesCard, StoryBackgroundsCard],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        The Lantern — where the engines that conjure pictures from prose are configured.
      </p>

      <!-- v4 ImagesTabContent wraps its cards in space-y-4; v5 had a single card
           and never needed the wrapper. With two, it is load-bearing. -->
      <div class="space-y-4">
        <qt-collapsible-card
          title="Image Profiles"
          description="Configure image generation providers and models"
          sectionId="image-profiles"
          [defaultOpen]="defaultOpen()"
          [forceOpen]="section() === 'image-profiles'"
        >
          <qt-image-profiles-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Story Backgrounds"
          description="Configure automatic story background image generation"
          sectionId="story-backgrounds"
          [defaultOpen]="defaultOpen()"
          [forceOpen]="section() === 'story-backgrounds'"
        >
          <qt-story-backgrounds-card />
        </qt-collapsible-card>
      </div>
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
