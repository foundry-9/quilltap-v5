import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  OnInit,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { RouterLink } from '@angular/router';

import { CoreClient } from '../core/core-client';
import type { AutonomousRoomSummary } from '../core/core-contract';
import { Icon } from '../ui/icon';
import {
  buildLabel,
  buildTooltip,
  filterBadgeRooms,
  selectBudgetReadout,
  type BudgetReadout,
} from './autonomous.logic';

const POLL_INTERVAL_MS = 5_000;
const TICK_INTERVAL_MS = 1_000;

/**
 * The toolbar run-state badges (v4 `components/layout/autonomous-room-badges.tsx`):
 * one compact handle per autonomous room currently idle/paused/running (a NARROWER
 * filter than the settings list's). Each abbreviates the chat (and parent project)
 * title, shows a single budget-remaining readout (priority tokens → turns → time),
 * and exposes an inline play/pause. A 5s poll mirrors the settings list; a local 1s
 * tick refreshes the time readout between polls for running, time-budgeted rooms
 * (client-side only — no polling amplification).
 */
@Component({
  selector: 'qt-autonomous-room-badges',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    @if (badgeRooms().length > 0) {
      <div class="qt-autonomous-badge-group">
        @for (room of badgeRooms(); track room.id) {
          <a
            [routerLink]="['/salon', room.id]"
            class="qt-autonomous-badge"
            [class]="
              room.runState === 'running'
                ? 'qt-autonomous-badge-running'
                : 'qt-autonomous-badge-idle'
            "
            [title]="tooltip(room)"
          >
            <span>{{ label(room) }}</span>
            @if (readout(room); as r) {
              <span>{{ r.label }}</span>
            }
            <button
              type="button"
              class="qt-autonomous-badge-button"
              (click)="toggle(room, $event)"
              [disabled]="busyChatId() === room.id"
              [attr.aria-label]="room.runState === 'running' ? 'Pause' : 'Resume'"
              [title]="room.runState === 'running' ? 'Pause' : 'Resume'"
            >
              <qt-icon
                [name]="room.runState === 'running' ? 'pause' : 'play'"
                class="w-2.5 h-2.5"
              />
            </button>
          </a>
        }
      </div>
    }
  `,
})
export class AutonomousRoomBadges implements OnInit {
  private readonly core = inject(CoreClient);
  private readonly destroyRef = inject(DestroyRef);

  private readonly allRooms = signal<AutonomousRoomSummary[]>([]);
  protected readonly busyChatId = signal<string | null>(null);
  protected readonly nowMs = signal(Date.now());

  protected readonly badgeRooms = computed(() => filterBadgeRooms(this.allRooms()));

  private readonly hasTimeBudgetedRunning = computed(() =>
    this.badgeRooms().some((r) => r.runState === 'running' && r.budgetMaxWallClockMs != null),
  );

  private tickHandle: ReturnType<typeof setInterval> | null = null;

  constructor() {
    // Start / stop the 1s tick as time-budgeted running rooms come and go
    // (client-side only — the readout refreshes between the 5s polls).
    effect(() => {
      const needsTick = this.hasTimeBudgetedRunning();
      if (needsTick && this.tickHandle === null) {
        this.tickHandle = setInterval(() => this.nowMs.set(Date.now()), TICK_INTERVAL_MS);
      } else if (!needsTick && this.tickHandle !== null) {
        clearInterval(this.tickHandle);
        this.tickHandle = null;
      }
    });
  }

  ngOnInit(): void {
    void this.refetch();
    const pollId = setInterval(() => void this.refetch(), POLL_INTERVAL_MS);
    this.destroyRef.onDestroy(() => {
      clearInterval(pollId);
      if (this.tickHandle !== null) clearInterval(this.tickHandle);
    });
  }

  protected label(room: AutonomousRoomSummary): string {
    return buildLabel(room);
  }

  protected readout(room: AutonomousRoomSummary): BudgetReadout {
    return selectBudgetReadout(room, this.nowMs());
  }

  protected tooltip(room: AutonomousRoomSummary): string {
    return buildTooltip(room, this.readout(room));
  }

  protected async toggle(room: AutonomousRoomSummary, event: Event): Promise<void> {
    event.preventDefault();
    event.stopPropagation();
    const isRunning = room.runState === 'running';
    const verb: 'pause' | 'resume' | 'start' = isRunning
      ? 'pause'
      : room.runState === 'paused'
        ? 'resume'
        : 'start';
    this.busyChatId.set(room.id);
    // Optimistically reflect the server's synchronous run-state flip.
    const optimistic = verb === 'pause' ? 'paused' : 'running';
    const previous = this.allRooms();
    this.allRooms.update((rooms) =>
      rooms.map((r) => (r.id === room.id ? { ...r, runState: optimistic } : r)),
    );
    try {
      if (verb === 'pause') await this.core.autonomousRoomPause(room.id);
      else if (verb === 'resume') await this.core.autonomousRoomResume(room.id);
      else await this.core.autonomousRoomStart(room.id);
    } catch {
      this.allRooms.set(previous);
    } finally {
      this.busyChatId.set(null);
      await this.refetch();
    }
  }

  private async refetch(): Promise<void> {
    try {
      this.allRooms.set(await this.core.listAutonomousRooms());
    } catch {
      // Best-effort; the next poll retries.
    }
  }
}
