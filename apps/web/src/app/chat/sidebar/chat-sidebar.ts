import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  type OnInit,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import type { ParticipantDetail, ParticipantStatusWire } from '../../core/core-contract';
import { Avatar } from '../../ui/avatar';
import { normalizeAvatarSrc } from '../../ui/avatar-stack';
import { CollapsibleCard } from '../../ui/collapsible-card';
import { Icon } from '../../ui/icon';
import {
  computePredictedTurnOrder,
  type TurnOrderEntry,
  type TurnOrderStatus,
  type TurnSelectionResult,
  type TurnState,
} from '../turn-order';
import { ChatSection, type ChatSectionState } from './chat-section';
import { OrganizeSection } from './organize-section';
import type { ConnectionProfileOption } from './participant-card';
import { ParticipantsSection } from './participants-section';
import { VisibilitySection, type VisibilityState } from './visibility-section';

const STORAGE_KEY = 'quilltap.chat-sidebar.collapsed';
const WIDTH_STORAGE_KEY = 'quilltap.chat-sidebar.width';

/** Expanded-sidebar width bounds (px). Default mirrors `--qt-chat-sidebar-width: 18rem`. */
const DEFAULT_WIDTH = 288;
const MIN_WIDTH = 240;
const MAX_WIDTH = 560;
/** Keep at least this much room for the chat pane when dragging. */
const MIN_CHAT_WIDTH = 360;
/** Keyboard nudge step (px). */
const WIDTH_KEY_STEP = 16;
/**
 * Below this container width (px) an inline expanded sidebar would starve the
 * chat (sidebar ~MIN_WIDTH + chat MIN_CHAT_WIDTH), so in a narrow pane the
 * sidebar defaults to the mini strip and expands as a click-away overlay
 * instead. Wide/full panes behave exactly as before (v4 `NARROW_PANE_WIDTH`).
 */
const NARROW_PANE_WIDTH = 640;

/** The single-open accordion's section ids (v4 `SectionId`). */
export type SidebarSectionId = 'participants' | 'chat' | 'visibility' | 'organize' | 'edit' | null;

/** Upper bound for the sidebar width given the viewport (leaves room for chat). */
function maxWidthForViewport(): number {
  if (typeof window === 'undefined') return MAX_WIDTH;
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, window.innerWidth - MIN_CHAT_WIDTH));
}

function clampWidth(value: number): number {
  return Math.round(Math.max(MIN_WIDTH, Math.min(maxWidthForViewport(), value)));
}

/** v4 `getInitialCollapsedState` — absent key ⇒ collapsed. */
function initialCollapsed(): boolean {
  if (typeof window === 'undefined') return true;
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored !== null ? stored === 'true' : true;
}

/** v4 `getInitialWidth`. */
function initialWidth(): number {
  if (typeof window === 'undefined') return DEFAULT_WIDTH;
  const stored = localStorage.getItem(WIDTH_STORAGE_KEY);
  const parsed = stored !== null ? parseInt(stored, 10) : NaN;
  return Number.isFinite(parsed) ? clampWidth(parsed) : DEFAULT_WIDTH;
}

/** v4 `getCollapsedPositionBadgeClass` (`ChatSidebar.tsx:1679`). */
function collapsedPositionBadgeClass(status: TurnOrderStatus): string {
  switch (status) {
    case 'generating':
      return 'qt-participant-position-generating';
    case 'next':
      return 'qt-participant-position-next';
    case 'queued':
      return 'qt-participant-position-queued';
    case 'eligible':
      return 'qt-participant-position-eligible';
    case 'user-turn':
      return 'qt-participant-position-user-turn';
    case 'spoken':
      return 'qt-participant-position-spoken';
    default:
      return '';
  }
}

/**
 * The Salon's right-hand chat sidebar (v4 `components/chat/ChatSidebar.tsx`):
 * a resizable, collapsible panel whose single-open accordion carries the cast
 * and the per-chat controls — and, since the episodic drift, the Story's Clock.
 *
 * Structure kept from v4: the collapsed mini-strip (avatars + turn badges +
 * pause), the drag/keyboard resizer on the inner edge, the persisted collapse
 * and width preferences (v4's exact localStorage keys), and the narrow-pane
 * overlay behaviour that keeps a split workspace pane from starving the chat.
 *
 * **Structural divergence (deliberate):** v4 renders the panel (or the strip)
 * as its own `div`; an Angular component adds a host element in between, which
 * would break `.qt-chat-layout`'s flex sizing. So the host element ITSELF
 * carries `qt-chat-sidebar` / `qt-chat-sidebar-collapsed` and the width style —
 * the DOM the CSS sees is v4's, one element instead of two, and the
 * ResizeObserver still measures `.qt-chat-layout` as the parent.
 */
@Component({
  selector: 'qt-chat-sidebar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    Avatar,
    Icon,
    CollapsibleCard,
    ParticipantsSection,
    ChatSection,
    VisibilitySection,
    OrganizeSection,
  ],
  host: {
    '[class.qt-chat-sidebar]': '!effectiveCollapsed()',
    '[class.qt-chat-sidebar-overlay]': 'isOverlay()',
    '[class.qt-chat-sidebar-collapsed]': 'effectiveCollapsed()',
    '[style.width.px]': 'effectiveCollapsed() ? null : width()',
  },
  template: `
    @if (effectiveCollapsed()) {
      <button
        type="button"
        class="qt-chat-sidebar-toggle"
        title="Expand chat sidebar"
        aria-label="Expand chat sidebar"
        (click)="onStripExpand()"
      >
        <qt-icon name="chevron-left" class="w-5 h-5" />
      </button>

      <button
        type="button"
        [class]="
          'qt-chat-sidebar-collapsed-pause ' +
          (isPaused() ? 'qt-chat-sidebar-collapsed-pause-paused' : '')
        "
        [title]="isPaused() ? 'Resume auto-responses' : 'Pause auto-responses'"
        [attr.aria-label]="isPaused() ? 'Resume auto-responses' : 'Pause auto-responses'"
        (click)="togglePause.emit()"
      >
        <qt-icon [name]="isPaused() ? 'play' : 'pause'" class="w-5 h-5" />
      </button>

      <div class="qt-chat-sidebar-collapsed-avatars">
        @for (participant of sortedParticipants(); track participant.id) {
          @let entry = turnEntry(participant.id);
          @let strip = stripState(participant, entry);
          <button
            type="button"
            [class]="strip.avatarClass"
            [title]="strip.title"
            [attr.aria-label]="strip.ariaLabel"
            (click)="onStripExpand()"
          >
            <div class="relative">
              <qt-avatar [name]="strip.name" [src]="strip.avatarUrl" size="sm" />
              @if (strip.status === 'silent') {
                <div class="qt-participant-status-overlay qt-participant-status-overlay-silent">
                  <qt-icon name="ban" class="w-2.5 h-2.5" />
                </div>
              }
              @if (strip.status === 'absent') {
                <div class="qt-participant-status-overlay qt-participant-status-overlay-absent">
                  <qt-icon name="log-out" class="w-2.5 h-2.5" />
                </div>
              }
            </div>
            @if (entry && entry.position != null && entry.position > 0) {
              <span
                [class]="'qt-chat-sidebar-collapsed-position-badge ' + strip.positionBadgeClass"
                >{{ entry.position }}</span
              >
            }
          </button>
        }
      </div>
    } @else {
      <div
        [class]="'qt-chat-sidebar-resizer ' + (isResizing() ? 'qt-chat-sidebar-resizer-active' : '')"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize chat sidebar"
        [attr.aria-valuenow]="width()"
        [attr.aria-valuemin]="minWidth"
        [attr.aria-valuemax]="maxWidth"
        tabindex="0"
        title="Drag to resize"
        (mousedown)="onResizeMouseDown($event)"
        (keydown)="onResizeKeyDown($event)"
      >
        <div class="qt-chat-sidebar-resizer-grip">
          <span></span>
          <span></span>
          <span></span>
        </div>
      </div>

      <div class="qt-chat-sidebar-header">
        <div class="flex items-center justify-between">
          <h3 class="qt-chat-sidebar-heading">Chat</h3>
          <button
            type="button"
            class="qt-chat-sidebar-toggle"
            title="Collapse chat sidebar"
            aria-label="Collapse chat sidebar"
            (click)="onExpandedCollapse()"
          >
            <qt-icon name="chevron-right" class="w-5 h-5" />
          </button>
        </div>
      </div>

      <div class="qt-chat-sidebar-list">
        <qt-collapsible-card
          title="Participants"
          [description]="participantsDescription()"
          [isOpen]="openSection() === 'participants'"
          (openChange)="setSection('participants', $event)"
        >
          <qt-participants-section
            [sortedParticipants]="sortedParticipants()"
            [turnOrder]="turnOrder()"
            [turnState]="turnState()"
            [turnSelectionResult]="turnSelectionResult()"
            [isGenerating]="isGenerating()"
            [isPaused]="isPaused()"
            [currentSpeakerId]="currentSpeakerId()"
            [userParticipantId]="userParticipantId()"
            [activeCharacterCount]="activeCharacterCount()"
            [impersonatingParticipantIds]="impersonatingParticipantIds()"
            [activeTypingParticipantId]="activeTypingParticipantId()"
            [isDangerousChat]="isDangerousChat()"
            [chatId]="chatId()"
            [connectionProfiles]="connectionProfiles()"
            (togglePause)="togglePause.emit()"
            (nudge)="nudge.emit($event)"
            (queue)="queue.emit($event)"
            (dequeue)="dequeue.emit($event)"
            (skip)="skip.emit()"
            (stopStreaming)="stopStreaming.emit()"
            (impersonate)="impersonate.emit($event)"
            (stopImpersonate)="stopImpersonate.emit($event)"
            (regenerateAvatar)="regenerateAvatar.emit($event)"
            (whisper)="whisper.emit($event)"
            (addCharacter)="addCharacter.emit()"
            (connectionProfileChange)="connectionProfileChange.emit($event)"
            (systemPromptChange)="systemPromptChange.emit($event)"
            (rebuildSystemPrompt)="rebuildSystemPrompt.emit($event)"
            (talkativenessChange)="talkativenessChange.emit($event)"
            (statusChange)="statusChange.emit($event)"
            (remove)="removeParticipant.emit($event)"
          />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Chat"
          [isOpen]="openSection() === 'chat'"
          (openChange)="setSection('chat', $event)"
        >
          <qt-chat-section
            [chatId]="chatId() ?? ''"
            [state]="chatSectionState()"
            [storyBackgroundsEnabled]="storyBackgroundsEnabled()"
            [regeneratingBackground]="regeneratingBackground()"
            [sectionOpen]="openSection() === 'chat'"
            (chatUpdated)="chatUpdated.emit()"
            (regenerateBackground)="regenerateBackground.emit()"
          />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Visibility"
          [isOpen]="openSection() === 'visibility'"
          (openChange)="setSection('visibility', $event)"
        >
          <qt-visibility-section
            [chatId]="chatId() ?? ''"
            [state]="visibilityState()"
            [turnSkippingApplies]="turnSkippingApplies()"
            [showAllWhispers]="showAllWhispers()"
            (toggleAllWhispers)="toggleAllWhispers.emit()"
            (chatUpdated)="chatUpdated.emit()"
          />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Organize"
          [isOpen]="openSection() === 'organize'"
          (openChange)="setSection('organize', $event)"
        >
          <qt-organize-section
            [chatId]="chatId() ?? ''"
            [isAutonomousRoom]="isAutonomousRoom()"
            (editEnclave)="editEnclave.emit()"
            (openState)="openState.emit()"
            (openGallery)="openGallery.emit()"
          />
        </qt-collapsible-card>
      </div>

      @if (turnSelectionResult()?.debug; as debug) {
        <div class="qt-chat-sidebar-debug">
          <details>
            <summary class="cursor-pointer hover:text-foreground">Turn Debug Info</summary>
            <div class="mt-2 space-y-1">
              <div>Reason: {{ turnSelectionResult()!.reason }}</div>
              <div>Cycle Complete: {{ turnSelectionResult()!.cycleComplete ? 'Yes' : 'No' }}</div>
              @if (debug.eligibleSpeakers.length > 0) {
                <div>Eligible: {{ debug.eligibleSpeakers.length }}</div>
              }
            </div>
          </details>
        </div>
      }
    }
  `,
})
export class ChatSidebar implements OnInit {
  private readonly host = inject(ElementRef<HTMLElement>);
  private readonly destroyRef = inject(DestroyRef);

  // --- Participants section ---
  readonly participants = input.required<ParticipantDetail[]>();
  readonly turnState = input.required<TurnState>();
  readonly turnSelectionResult = input<TurnSelectionResult | null>(null);
  readonly isGenerating = input(false);
  readonly isPaused = input(false);
  readonly userParticipantId = input<string | null>(null);
  readonly respondingParticipantId = input<string | null>(null);
  readonly impersonatingParticipantIds = input<string[]>([]);
  readonly activeTypingParticipantId = input<string | null>(null);
  readonly isDangerousChat = input(false);
  readonly chatId = input<string | null>(null);
  /** The user's connection profiles, threaded to each participant card. */
  readonly connectionProfiles = input<ConnectionProfileOption[]>([]);

  // --- Chat section ---
  readonly chatSectionState = input.required<ChatSectionState>();
  /** v4 `chatControls.storyBackgroundsEnabled` — gates the regenerate entry. */
  readonly storyBackgroundsEnabled = input(false);
  readonly regeneratingBackground = input(false);

  // --- Visibility section ---
  readonly visibilityState = input.required<VisibilityState>();
  /** v4 gates the Turn Skipping row on `qualifiesForTurnSkipping` AND `isMultiChar`. */
  readonly turnSkippingApplies = input(false);
  readonly showAllWhispers = input(false);
  readonly toggleAllWhispers = output<void>();

  // --- Organize section ---
  readonly isAutonomousRoom = input(false);
  readonly editEnclave = output<void>();
  readonly openState = output<void>();
  readonly openGallery = output<void>();

  readonly chatUpdated = output<void>();
  readonly regenerateBackground = output<void>();
  readonly togglePause = output<void>();
  readonly nudge = output<string>();
  readonly queue = output<string>();
  readonly dequeue = output<string>();
  readonly skip = output<void>();
  readonly stopStreaming = output<void>();
  readonly impersonate = output<string>();
  readonly stopImpersonate = output<string>();
  readonly regenerateAvatar = output<string>();
  /** A cast member's Whisper button (v4 `onWhisper` → `SalonView.handleWhisper`). */
  readonly whisper = output<string>();
  /** The cast footer's Add Character (v4 `onAddCharacter` → `AddCharacterDialog`). */
  readonly addCharacter = output<void>();
  /** The per-card participant controls (v4 `SalonView`'s `useChatControls` handlers). */
  readonly connectionProfileChange = output<{
    participantId: string;
    profileId: string | null;
    controlledBy: 'llm' | 'user';
  }>();
  readonly systemPromptChange = output<{ participantId: string; promptId: string | null }>();
  readonly rebuildSystemPrompt = output<string>();
  readonly talkativenessChange = output<{ participantId: string; value: number }>();
  readonly statusChange = output<{ participantId: string; status: ParticipantStatusWire }>();
  readonly removeParticipant = output<string>();

  protected readonly minWidth = MIN_WIDTH;
  protected readonly maxWidth = MAX_WIDTH;

  protected readonly isCollapsed = signal(initialCollapsed());
  protected readonly openSection = signal<SidebarSectionId>('participants');
  protected readonly width = signal(initialWidth());
  protected readonly isResizing = signal(false);
  /** The container (`.qt-chat-layout`) width, or null before the first measure. */
  private readonly containerWidth = signal<number | null>(null);
  protected readonly narrowOpen = signal(false);

  protected readonly isNarrow = computed(() => {
    const w = this.containerWidth();
    return w != null && w < NARROW_PANE_WIDTH;
  });
  protected readonly effectiveCollapsed = computed(() =>
    this.isNarrow() ? !this.narrowOpen() : this.isCollapsed(),
  );
  protected readonly isOverlay = computed(() => this.isNarrow() && this.narrowOpen());

  ngOnInit(): void {
    // v4 measures the sidebar's PARENT (`.qt-chat-layout`), which both the panel
    // and the strip share, so the observer survives collapse/expand. Falls back
    // to a one-shot measurement where ResizeObserver is unavailable (jsdom).
    const parent = this.host.nativeElement.parentElement;
    if (!parent) return;
    if (typeof ResizeObserver === 'undefined') {
      this.containerWidth.set(parent.getBoundingClientRect().width || null);
      return;
    }
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) this.containerWidth.set(entry.contentRect.width);
    });
    ro.observe(parent);
    this.containerWidth.set(parent.getBoundingClientRect().width);
    this.destroyRef.onDestroy(() => ro.disconnect());
  }

  constructor() {
    // Leaving narrow mode drops any transient overlay state (v4 does this during
    // render; an effect is Angular's equivalent seam).
    effect(() => {
      if (!this.isNarrow() && this.narrowOpen()) {
        this.narrowOpen.set(false);
      }
    });

    // Overlay: a click outside the panel, or Escape, collapses it to the strip.
    effect((onCleanup) => {
      if (!this.isOverlay()) return;
      const onPointerDown = (e: PointerEvent) => {
        if (!this.host.nativeElement.contains(e.target as Node)) this.narrowOpen.set(false);
      };
      const onKeyDown = (e: KeyboardEvent) => {
        if (e.key === 'Escape') this.narrowOpen.set(false);
      };
      document.addEventListener('pointerdown', onPointerDown, true);
      document.addEventListener('keydown', onKeyDown);
      onCleanup(() => {
        document.removeEventListener('pointerdown', onPointerDown, true);
        document.removeEventListener('keydown', onKeyDown);
      });
    });
  }

  // -------------------------------------------------------------------------
  // Turn-order projections (v4 `ChatSidebar`'s useMemos)
  // -------------------------------------------------------------------------

  protected readonly turnOrder = computed<TurnOrderEntry[]>(() =>
    computePredictedTurnOrder({
      participants: this.participants(),
      turnState: this.turnState(),
      turnSelectionResult: this.turnSelectionResult(),
      isGenerating: this.isGenerating(),
      respondingParticipantId: this.respondingParticipantId(),
      userParticipantId: this.userParticipantId(),
    }),
  );

  private readonly turnOrderMap = computed(() => {
    const map = new Map<string, TurnOrderEntry>();
    for (const entry of this.turnOrder()) map.set(entry.participantId, entry);
    return map;
  });

  protected readonly sortedParticipants = computed(() => {
    const orderIndex = new Map<string, number>();
    this.turnOrder().forEach((entry, index) => orderIndex.set(entry.participantId, index));
    return [...this.participants()].sort(
      (a, b) => (orderIndex.get(a.id) ?? 999) - (orderIndex.get(b.id) ?? 999),
    );
  });

  protected readonly currentSpeakerId = computed(() => {
    if (this.isGenerating()) {
      if (this.respondingParticipantId()) return this.respondingParticipantId();
      if (this.turnState().lastSpeakerId) return this.turnState().lastSpeakerId;
    }
    return this.turnSelectionResult()?.nextSpeakerId ?? null;
  });

  protected readonly activeCharacterCount = computed(
    () =>
      this.participants().filter(
        (p) =>
          p.type === 'CHARACTER' &&
          p.isActive &&
          p.controlledBy !== 'user' &&
          p.id !== this.userParticipantId(),
      ).length,
  );

  /** v4 `${n} character${n !== 1 ? 's' : ''}`. */
  protected readonly participantsDescription = computed(() => {
    const n = this.activeCharacterCount();
    return `${n} character${n !== 1 ? 's' : ''}`;
  });

  protected turnEntry(participantId: string): TurnOrderEntry | undefined {
    return this.turnOrderMap().get(participantId);
  }

  /** The collapsed strip's per-avatar derivation (v4 `CollapsedStrip`'s map body). */
  protected stripState(participant: ParticipantDetail, entry: TurnOrderEntry | undefined) {
    const isUserParticipant = participant.id === this.userParticipantId();
    const isInactive = entry?.status === 'inactive';
    const isUserTurn = this.turnSelectionResult()?.nextSpeakerId === null && !this.isGenerating();
    const isCurrentTurn =
      this.currentSpeakerId() === participant.id || (isUserParticipant && isUserTurn);
    const isActivelyGenerating =
      this.currentSpeakerId() === participant.id && this.isGenerating();

    const name = participant.character?.name || 'Unknown';
    const status = participant.status || 'active';

    const avatarClasses = ['qt-chat-sidebar-collapsed-avatar'];
    if (isInactive) {
      avatarClasses.push('qt-chat-sidebar-collapsed-avatar-inactive');
    } else if (isCurrentTurn || isActivelyGenerating) {
      avatarClasses.push('qt-chat-sidebar-collapsed-avatar-active');
    }
    if (isActivelyGenerating) {
      avatarClasses.push('qt-chat-sidebar-collapsed-avatar-streaming');
    }

    const statusLabel = status !== 'active' ? ` [${status}]` : '';
    return {
      name,
      status,
      avatarUrl: normalizeAvatarSrc(participant.character?.avatarUrl),
      avatarClass: avatarClasses.join(' '),
      positionBadgeClass: entry ? collapsedPositionBadgeClass(entry.status) : '',
      title: `${name}${statusLabel}${isCurrentTurn ? ' (current turn)' : ''}${
        entry?.position ? ` (#${entry.position})` : ''
      }`,
      ariaLabel: `${name}${statusLabel} - click to expand sidebar`,
    };
  }

  // -------------------------------------------------------------------------
  // Collapse / width (v4's persisted preferences)
  // -------------------------------------------------------------------------

  protected setSection(id: Exclude<SidebarSectionId, null>, open: boolean): void {
    this.openSection.set(open ? id : null);
  }

  /** In a narrow pane, expand drives the transient overlay (v4 `handleStripExpand`). */
  protected onStripExpand(): void {
    if (this.isNarrow()) {
      this.narrowOpen.set(true);
      return;
    }
    this.isCollapsed.set(false);
    localStorage.setItem(STORAGE_KEY, 'false');
  }

  /** v4 `handleExpandedCollapse` — the overlay closes without touching the preference. */
  protected onExpandedCollapse(): void {
    if (this.isNarrow()) {
      this.narrowOpen.set(false);
      return;
    }
    this.isCollapsed.update((prev) => {
      const next = !prev;
      localStorage.setItem(STORAGE_KEY, String(next));
      return next;
    });
  }

  /**
   * Drag-to-resize from the inner (left) edge (v4 `handleResizeMouseDown`):
   * capture the panel's right edge, then track the cursor against it. Dragging
   * left (toward the chat) widens the sidebar.
   */
  protected onResizeMouseDown(e: MouseEvent): void {
    e.preventDefault();
    const rightEdge = this.host.nativeElement.getBoundingClientRect().right ?? null;
    if (rightEdge === null) return;

    this.isResizing.set(true);
    let latest = this.width();

    const onMouseMove = (moveEvent: MouseEvent) => {
      latest = clampWidth(rightEdge - moveEvent.clientX);
      this.width.set(latest);
    };
    const onMouseUp = () => {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      this.isResizing.set(false);
      localStorage.setItem(WIDTH_STORAGE_KEY, String(latest));
    };

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }

  /** v4 `handleResizeKeyDown` — ArrowLeft widens, ArrowRight narrows. */
  protected onResizeKeyDown(event: KeyboardEvent): void {
    let next: number | null = null;
    switch (event.key) {
      case 'ArrowLeft':
        next = this.width() + WIDTH_KEY_STEP;
        break;
      case 'ArrowRight':
        next = this.width() - WIDTH_KEY_STEP;
        break;
      case 'Home':
        next = MIN_WIDTH;
        break;
      case 'End':
        next = maxWidthForViewport();
        break;
      default:
        return;
    }
    event.preventDefault();
    const clamped = clampWidth(next);
    this.width.set(clamped);
    localStorage.setItem(WIDTH_STORAGE_KEY, String(clamped));
  }
}
