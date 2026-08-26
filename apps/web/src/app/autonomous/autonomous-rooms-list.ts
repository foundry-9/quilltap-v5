import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { RealtimeService } from '../core/realtime.service';
import type { AutonomousRoomSummary } from '../core/core-contract';
import { RouterLink } from '@angular/router';
import { EditEnclaveModal } from './edit-enclave-modal';
import { filterManagementRooms, summarizeBudget } from './autonomous.logic';

type ActionVerb = 'start' | 'pause' | 'stop' | 'resume';

/** Fallback poll cadence, used only while the realtime channel is down. */
const POLL_INTERVAL_MS = 5_000;

function badgeClass(state: string | null): string {
  switch (state) {
    case 'running':
      return 'qt-badge-autonomous-running';
    case 'idle':
      return 'qt-badge-autonomous-idle';
    case 'paused':
      return 'qt-badge-autonomous-paused';
    case 'stopped':
      return 'qt-badge-autonomous-stopped';
    case 'budgetExhausted':
      return 'qt-badge-autonomous-budget';
    case 'error':
      return 'qt-badge-autonomous-error';
    default:
      return 'qt-badge-autonomous-stopped';
  }
}

function formatTimestamp(iso: string | null): string {
  if (!iso) return '—';
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

/**
 * The Scheduled-Autonomous-Rooms management list (v4
 * `components/tools/autonomous-rooms-card.tsx`): every autonomous
 * room, filtered so cron rooms always appear and ad-hoc rooms appear only while
 * idle/running/paused. Each row shows the run state, cron/manual badge, last/next
 * run, the budget summary, and Start/Pause/Resume/Stop/Edit controls — the run
 * state is patched optimistically the instant a button is clicked (the server
 * flips it synchronously), then the next read reconciles.
 *
 * Same reading as the toolbar badges: pushed by the `autonomousRooms` topic,
 * with the original 5 s cadence held in reserve for a dropped channel (v4
 * `f3892158d`).
 */
@Component({
  selector: 'qt-autonomous-rooms-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, EditEnclaveModal],
  template: `
    @if (loading()) {
      <div class="qt-text-secondary">Loading autonomous rooms…</div>
    } @else if (loadError()) {
      <div class="qt-alert-error">Failed to load autonomous rooms.</div>
    } @else if (rooms().length === 0) {
      <div class="qt-text-small qt-text-muted">
        No active autonomous rooms. Cron-scheduled rooms always appear here; ad-hoc rooms appear
        while idle, running, or paused.
      </div>
    } @else {
      <div class="space-y-3">
        @for (room of rooms(); track room.id) {
          <div class="border qt-border-default rounded p-3">
            <div class="flex items-start justify-between gap-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <a
                    [routerLink]="['/salon', room.id]"
                    class="font-medium text-foreground hover:underline truncate"
                  >
                    {{ room.title || '(untitled autonomous room)' }}
                  </a>
                  <span class="qt-badge text-xs" [class]="badgeClass(room.runState)">
                    {{ room.runState ?? 'idle' }}
                  </span>
                  @if (room.scheduleCron) {
                    <span class="qt-badge qt-badge-auto text-xs"
                      >cron: {{ room.scheduleCron }}</span
                    >
                  } @else {
                    <span class="qt-badge qt-badge-manual text-xs">manual</span>
                  }
                </div>
                <div class="qt-text-small mt-1">
                  <span>Last run: {{ formatTimestamp(room.scheduleLastRunAt) }}</span>
                  <span class="mx-2">·</span>
                  <span>Next run: {{ formatTimestamp(room.scheduleNextRunAt) }}</span>
                </div>
                <div class="qt-text-small">{{ summarize(room) }}</div>
                @if (room.runStateMessage) {
                  <div class="qt-text-small qt-text-muted italic mt-1">
                    {{ room.runStateMessage }}
                  </div>
                }
              </div>

              <div class="flex flex-col gap-1 shrink-0">
                <button
                  class="qt-button-secondary text-xs px-2 py-1"
                  (click)="onPrimary(room)"
                  [disabled]="busyChatId() === room.id || (!isRunning(room) && !canStart(room))"
                >
                  {{ primaryLabel(room) }}
                </button>
                <button
                  class="qt-button-secondary text-xs px-2 py-1"
                  (click)="act(room.id, 'stop')"
                  [disabled]="busyChatId() === room.id || room.runState === 'stopped'"
                >
                  Stop
                </button>
                <button
                  class="qt-button-secondary text-xs px-2 py-1"
                  (click)="openEdit(room)"
                  [disabled]="busyChatId() === room.id"
                  title="Edit this enclave’s schedule, budget, and visibility"
                >
                  Edit
                </button>
              </div>
            </div>
          </div>
        }
      </div>
    }

    @if (editRoom(); as er) {
      <qt-edit-enclave-modal
        [chatId]="er.id"
        [currentTitle]="er.title"
        (close)="editRoom.set(null)"
        (saved)="onEditSaved()"
      />
    }
  `,
})
export class AutonomousRoomsList implements OnInit {
  private readonly core = inject(CoreClient);
  private readonly realtime = inject(RealtimeService);

  constructor() {
    // Live path: run-state transitions publish `autonomousRooms`.
    this.realtime.onTopic('autonomousRooms', () => void this.refetch());
    // Fallback: the original 5 s cadence, while the channel is down.
    this.realtime.fallbackPoll(POLL_INTERVAL_MS, () => void this.refetch());
  }

  protected readonly badgeClass = badgeClass;
  protected readonly formatTimestamp = formatTimestamp;

  private readonly allRooms = signal<AutonomousRoomSummary[]>([]);
  protected readonly loading = signal(true);
  protected readonly loadError = signal(false);
  protected readonly busyChatId = signal<string | null>(null);
  protected readonly editRoom = signal<{ id: string; title: string } | null>(null);

  protected readonly rooms = computed(() => filterManagementRooms(this.allRooms()));

  ngOnInit(): void {
    void this.refetch();
  }

  protected summarize(room: AutonomousRoomSummary): string {
    return summarizeBudget(room);
  }

  protected isRunning(room: AutonomousRoomSummary): boolean {
    return room.runState === 'running';
  }

  protected canStart(room: AutonomousRoomSummary): boolean {
    const s = room.runState;
    return s === 'idle' || s === 'paused' || s === 'budgetExhausted' || s === 'stopped';
  }

  protected primaryLabel(room: AutonomousRoomSummary): string {
    if (this.isRunning(room)) return 'Pause';
    return room.runState === 'paused' || room.runState === 'budgetExhausted' ? 'Resume' : 'Start';
  }

  protected onPrimary(room: AutonomousRoomSummary): void {
    const verb: ActionVerb = this.isRunning(room)
      ? 'pause'
      : room.runState === 'paused'
        ? 'resume'
        : 'start';
    void this.act(room.id, verb);
  }

  protected async act(chatId: string, verb: ActionVerb): Promise<void> {
    this.busyChatId.set(chatId);
    // Optimistically reflect the server's synchronous run-state flip.
    const optimistic = verb === 'pause' ? 'paused' : verb === 'stop' ? 'stopped' : 'running';
    const previous = this.allRooms();
    this.allRooms.update((rooms) =>
      rooms.map((r) => (r.id === chatId ? { ...r, runState: optimistic } : r)),
    );
    try {
      switch (verb) {
        case 'start':
          await this.core.autonomousRoomStart(chatId);
          break;
        case 'pause':
          await this.core.autonomousRoomPause(chatId);
          break;
        case 'stop':
          await this.core.autonomousRoomStop(chatId);
          break;
        case 'resume':
          await this.core.autonomousRoomResume(chatId);
          break;
      }
    } catch {
      // Roll back the optimistic patch on failure.
      this.allRooms.set(previous);
    } finally {
      this.busyChatId.set(null);
      await this.refetch();
    }
  }

  protected openEdit(room: AutonomousRoomSummary): void {
    this.editRoom.set({ id: room.id, title: room.title });
  }

  protected onEditSaved(): void {
    this.editRoom.set(null);
    void this.refetch();
  }

  private async refetch(): Promise<void> {
    try {
      const rooms = await this.core.listAutonomousRooms();
      this.allRooms.set(rooms);
      this.loadError.set(false);
    } catch {
      this.loadError.set(true);
    } finally {
      this.loading.set(false);
    }
  }
}
