import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { AutonomousRoomDefaults } from '../../../autonomous/autonomous-room-defaults';
import { AutonomousRoomsList } from '../../../autonomous/autonomous-rooms-list';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { DataRetentionSettings } from './data-retention-settings';

/**
 * The Settings → Chat tab (v4 `components/settings/tabs/ChatTabContent.tsx`,
 * subsystem `salon`). This lane fits out the Data Retention card (P4.d3, in v4's
 * position ahead of Autonomous Rooms) plus the two autonomous cards — the user
 * defaults (`autonomous-rooms`) and the scheduled-rooms management list
 * (`autonomous-room-schedules`) — over `CollapsibleCard` with v4's card
 * titles/descriptions + `sectionId`s (the `?section=` deep link) ported verbatim.
 *
 * DEFERRED LOUDLY (named, not silent): v4's other Chat-tab cards — Composition
 * Mode, Composer, Auto-Scroll, Text Replacement, Token Display, Context
 * Compression, Memory Cascade, Image Description, Automation, Agent Mode,
 * Thinking / Reasoning, Answer Confirmation, and Dangerous Content — each land in
 * their own settings order. They are enumerated in the "not yet fitted out"
 * placeholder below rather than rendered as dead cards.
 */
@Component({
  selector: 'qt-settings-chat',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, AutonomousRoomDefaults, AutonomousRoomsList, DataRetentionSettings],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        The Salon's standing house rules — how conversations compose themselves, keep their books,
        and, of late, run on without you.
      </p>

      <div class="space-y-4">
        <!-- P4.d3: v4 places Data Retention between "Dangerous Content" (still a
             loud deferral below) and "Autonomous Rooms". -->
        <qt-collapsible-card
          title="Data Retention"
          description="How long inactive chats keep their regenerable working data"
          sectionId="data-retention"
          [defaultOpen]="defaultOpen()"
          [forceOpen]="section() === 'data-retention'"
        >
          <qt-data-retention-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Autonomous Rooms"
          description="Defaults for private character-to-character rooms"
          sectionId="autonomous-rooms"
          [forceOpen]="section() === 'autonomous-rooms'"
        >
          <qt-autonomous-room-defaults />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Scheduled Autonomous Rooms"
          description="Pause, resume, or stop scheduled rooms (and any ad-hoc room currently running)"
          sectionId="autonomous-room-schedules"
          [forceOpen]="section() === 'autonomous-room-schedules'"
        >
          <qt-autonomous-rooms-list />
        </qt-collapsible-card>

        <!-- LOUD DEFERRAL: the remaining v4 Chat-tab cards, each awaiting its own order. -->
        <div class="rounded-xl border qt-border-default qt-bg-card/40 p-4">
          <p class="font-medium text-foreground">
            The rest of the Chat cabinet is not yet fitted out
          </p>
          <p class="qt-text-small qt-text-muted mt-1">
            These dials are ported elsewhere and will be hung here in their own rounds — for the
            moment they remain in the workshop:
          </p>
          <p class="qt-text-xs qt-text-muted mt-2 italic">
            Composition Mode · Composer · Auto-Scroll · Text Replacement · Token Display · Context
            Compression · Memory Cascade · Image Description · Automation · Agent Mode · Thinking /
            Reasoning · Answer Confirmation · Dangerous Content.
          </p>
        </div>
      </div>
    </div>
  `,
})
export class ChatTab implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly queryParams = toSignal(this.route.queryParamMap, { requireSync: true });

  protected readonly section = computed(() => this.queryParams().get('section'));
  protected readonly defaultOpen = signal(false);

  ngOnInit(): void {
    // Open the first card by default (bound inputs have resolved by ngOnInit).
    this.defaultOpen.set(true);
  }
}
