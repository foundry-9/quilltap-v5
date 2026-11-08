import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  Directive,
  ElementRef,
  TemplateRef,
  afterNextRender,
  contentChild,
  inject,
  input,
  output,
  signal,
  viewChildren,
} from '@angular/core';
import { RouterLink } from '@angular/router';

import { Icon } from '../ui/icon';
import { Tooltip } from '../ui/tooltip';
import { CopyChatIdButton } from '../ui/copy-chat-id-button';
import {
  BREADCRUMB_DENSITIES,
  TITLE_MEASURE_MAX_PX,
  abbreviateName,
  chooseDensity,
  type BreadcrumbDensity,
} from './breadcrumb-density';

/** One LLM character link in the breadcrumb (v4 `ChatHeaderCharacter`). */
export interface BreadcrumbCharacter {
  id: string;
  name: string;
  /** The resolved avatar, or `null` for the initial disc (and under quick-hide). */
  avatarUrl: string | null;
}

/** The visible story background, as the thumbnail button opens it. */
export interface BreadcrumbStoryBackground {
  src: string;
  filename: string | null;
  fileId: string | null;
}

/**
 * Marks the Concierge badge the header hands the breadcrumb:
 * `<ng-template qtBreadcrumbBadge>…</ng-template>` inside
 * `<qt-conversation-breadcrumb>`.
 *
 * v4 passes the badge as a ReactNode prop and renders it in the visible row AND
 * in each measuring copy (its width counts toward every density). A plain
 * `<ng-content>` projects exactly once, so the badge is projected as a TEMPLATE
 * and stamped into all four rows; its markup and presentation table stay in
 * `conversation-header.ts`.
 */
@Directive({ selector: 'ng-template[qtBreadcrumbBadge]' })
export class BreadcrumbBadge {
  readonly templateRef = inject<TemplateRef<unknown>>(TemplateRef);
}

interface RowContext {
  $implicit: BreadcrumbDensity;
  /** The invisible measuring copy: no tooltips, no focusable links, title capped */
  measuring: boolean;
}

/**
 * The Salon header's left-hand breadcrumb (v4
 * `components/chat/ChatHeaderBreadcrumb.tsx`, `3c41cf3da`, pinned at
 * `624d0d25c`): project / characters / story background / Concierge badge /
 * chat title / copy-ID. A crowded chat has more names than the header has room
 * for, so the project and character links step down through three densities
 * until the row fits:
 *
 *   full    — avatar + full name
 *   abbrev  — avatar + short abbreviation
 *   avatar  — avatar alone
 *
 * The compact forms carry a Quilltap tooltip with the full name. The story
 * background and the title never compact; the title truncates as a last resort.
 *
 * Each density is laid out once in an invisible measuring copy so the choice
 * never depends on the row's own (already compacted) width. The host IS v4's
 * container (`hidden md:flex …`): its `clientWidth` is the room available, and
 * one `ResizeObserver` watches it and the three copies.
 *
 * Links follow v5's routes: the project → `/prospero/<id>`, a character →
 * `/characters/<id>?tab=conversations` (v4 `/aurora/<id>/view?tab=…`; the
 * `mapResultUrl` precedent), the title → `/salon/<id>`.
 */
@Component({
  selector: 'qt-conversation-breadcrumb',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet, RouterLink, Icon, Tooltip, CopyChatIdButton],
  host: {
    class: 'hidden md:flex relative items-center gap-2 text-sm min-w-0 w-full overflow-hidden',
  },
  template: `
    <ng-container
      [ngTemplateOutlet]="row"
      [ngTemplateOutletContext]="{ $implicit: density(), measuring: false }"
    />
    @for (d of densities; track d) {
      <div
        #measure
        class="absolute left-0 top-0 flex items-center gap-2 w-max invisible pointer-events-none whitespace-nowrap"
        aria-hidden="true"
        inert
      >
        <ng-container
          [ngTemplateOutlet]="row"
          [ngTemplateOutletContext]="{ $implicit: d, measuring: true }"
        />
      </div>
    }

    <ng-template #row let-density let-measuring="measuring">
      <!-- Clips only once the title is down to its floor -->
      @if (project() || characters().length > 0) {
        <span class="flex items-center gap-2 min-w-0 overflow-hidden">
          @if (project(); as p) {
            <ng-template #projectLink>
              <a
                [routerLink]="['/prospero', p.id]"
                class="inline-flex items-center gap-1.5 qt-text-secondary hover:text-foreground transition-colors flex-shrink-0"
                [attr.aria-label]="density !== 'full' ? 'Project: ' + p.name : null"
                [attr.tabindex]="measuring ? -1 : null"
              >
                <qt-icon name="folder" class="w-4 h-4" />
                @if (density === 'full') {
                  <span>{{ p.name }}</span>
                } @else if (density === 'abbrev') {
                  <span aria-hidden="true">{{ abbreviate(p.name) }}</span>
                }
              </a>
            </ng-template>
            @if (density !== 'full' && !measuring) {
              <qt-tooltip placement="bottom" class="flex-shrink-0" [contentTemplate]="projectTip">
                <ng-container [ngTemplateOutlet]="projectLink" />
              </qt-tooltip>
              <ng-template #projectTip>
                <div class="qt-tooltip-body">
                  <p class="qt-tooltip-title">{{ p.name }}</p>
                  <p class="qt-tooltip-hint">Click to open project</p>
                </div>
              </ng-template>
            } @else {
              <ng-container [ngTemplateOutlet]="projectLink" />
            }
            <span class="qt-text-muted flex-shrink-0">/</span>
          }
          @for (character of characters(); track character.id) {
            <span class="contents">
              <ng-template #characterLink>
                <a
                  [routerLink]="['/characters', character.id]"
                  [queryParams]="{ tab: 'conversations' }"
                  class="inline-flex items-center gap-1.5 qt-text-secondary hover:text-foreground transition-colors flex-shrink-0"
                  [attr.aria-label]="density !== 'full' ? character.name : null"
                  [attr.tabindex]="measuring ? -1 : null"
                >
                  @if (character.avatarUrl) {
                    <img
                      [src]="character.avatarUrl"
                      alt=""
                      class="w-5 h-5 rounded-full object-cover flex-shrink-0"
                    />
                  } @else {
                    <span
                      class="w-5 h-5 rounded-full qt-bg-muted flex items-center justify-center flex-shrink-0"
                    >
                      <span class="text-xs font-medium qt-text-secondary">{{
                        initial(character.name)
                      }}</span>
                    </span>
                  }
                  @if (density === 'full') {
                    <span>{{ character.name }}</span>
                  } @else if (density === 'abbrev') {
                    <span aria-hidden="true">{{ abbreviate(character.name) }}</span>
                  }
                </a>
              </ng-template>
              @if (density !== 'full' && !measuring) {
                <qt-tooltip
                  placement="bottom"
                  class="flex-shrink-0"
                  [contentTemplate]="characterTip"
                >
                  <ng-container [ngTemplateOutlet]="characterLink" />
                </qt-tooltip>
                <ng-template #characterTip>
                  <div class="qt-tooltip-body">
                    <p class="qt-tooltip-title">{{ character.name }}</p>
                    <p class="qt-tooltip-hint">Click to open character</p>
                  </div>
                </ng-template>
              } @else {
                <ng-container [ngTemplateOutlet]="characterLink" />
              }
              <span class="qt-text-muted flex-shrink-0">/</span>
            </span>
          }
        </span>
      }
      @if (storyBackground(); as bg) {
        <ng-template #backgroundButton>
          <button
            type="button"
            class="flex-shrink-0 rounded overflow-hidden hover:ring-2 hover:ring-primary/50 transition-all"
            aria-label="Display story background"
            [attr.tabindex]="measuring ? -1 : null"
            (click)="measuring ? null : openStoryBackground.emit(bg)"
          >
            <img [src]="bg.src" alt="" class="w-8 h-5 object-cover" />
          </button>
        </ng-template>
        @if (measuring) {
          <ng-container [ngTemplateOutlet]="backgroundButton" />
        } @else {
          <qt-tooltip content="Click to display background" placement="bottom" class="flex-shrink-0">
            <ng-container [ngTemplateOutlet]="backgroundButton" />
          </qt-tooltip>
        }
      }
      @if (badge(); as b) {
        <ng-container [ngTemplateOutlet]="b.templateRef" />
      }
      <ng-template #titleLink>
        <a
          [routerLink]="['/salon', chatId()]"
          class="qt-text-primary truncate min-w-0 hover:text-foreground transition-colors"
          [style.max-width.px]="measuring ? titleMeasureMaxPx : null"
          [style.display]="measuring ? 'inline-block' : null"
          [attr.tabindex]="measuring ? -1 : null"
          >{{ title() }}</a
        >
      </ng-template>
      @if (measuring) {
        <ng-container [ngTemplateOutlet]="titleLink" />
      } @else {
        <qt-tooltip [content]="title()" placement="bottom" class="min-w-32 shrink-[1000]">
          <ng-container [ngTemplateOutlet]="titleLink" />
        </qt-tooltip>
      }
      <qt-copy-chat-id-button [chatId]="chatId()" variant="inline" />
    </ng-template>
  `,
})
export class ConversationBreadcrumb {
  readonly chatId = input.required<string>();
  /** The displayed title (the header supplies v5's `Untitled chat` fallback). */
  readonly title = input.required<string>();
  readonly project = input<{ id: string; name: string } | null>(null);
  readonly characters = input<BreadcrumbCharacter[]>([]);
  /** Thumbnail of the story background, when one is visible */
  readonly storyBackground = input<BreadcrumbStoryBackground | null>(null);
  /** The thumbnail was clicked — carries the background it shows. */
  readonly openStoryBackground = output<BreadcrumbStoryBackground>();

  protected readonly badge = contentChild(BreadcrumbBadge);
  protected readonly densities = BREADCRUMB_DENSITIES;
  protected readonly titleMeasureMaxPx = TITLE_MEASURE_MAX_PX;
  protected readonly density = signal<BreadcrumbDensity>('full');

  private readonly measures = viewChildren<ElementRef<HTMLElement>>('measure');
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;

  constructor() {
    const destroyRef = inject(DestroyRef);
    // v4's useLayoutEffect: measure once the copies exist, then on every resize
    // of the container or of any copy (a copy grows when a name or the badge
    // changes, so the choice follows content as well as room).
    afterNextRender(() => {
      this.recompute();
      if (typeof ResizeObserver === 'undefined') return;
      const observer = new ResizeObserver(() => this.recompute());
      observer.observe(this.host);
      for (const el of this.measures()) observer.observe(el.nativeElement);
      destroyRef.onDestroy(() => observer.disconnect());
    });
  }

  protected abbreviate(name: string): string {
    return abbreviateName(name);
  }

  /** v4 `character.name.charAt(0).toUpperCase()` — a UTF-16 unit, as v4 reads it. */
  protected initial(name: string): string {
    return name.charAt(0).toUpperCase();
  }

  private recompute(): void {
    const available = this.host.clientWidth;
    // Hidden below md — nothing to fit.
    if (available === 0) return;
    // The copies render in BREADCRUMB_DENSITIES order, so index i is density i.
    const copies = this.measures();
    const widths = {} as Record<BreadcrumbDensity, number>;
    BREADCRUMB_DENSITIES.forEach((d, i) => {
      widths[d] = copies[i]?.nativeElement.scrollWidth ?? 0;
    });
    this.density.set(chooseDensity(available, widths));
  }
}
