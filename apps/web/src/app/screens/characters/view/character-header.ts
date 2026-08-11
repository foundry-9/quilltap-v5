import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import type {
  CharacterDetail,
  CharacterGroupBadge,
  CharacterStats,
} from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { characterAvatarSrc } from '../characters.api';

interface StatItem {
  key: string;
  value: string | number;
  label: string;
  tip: string;
}

/**
 * The header card (v4 `app/aurora/[id]/view/components/CharacterHeader.tsx`):
 * avatar, name/title/pronouns/aliases, the stat line + group badges, the three
 * inline toggles (favorite / Carina / controlled-by), and the action column
 * (Start Chat, Convert to NPC/Character, plus three disabled deferrals —
 * Non-Quilltap Prompt, Refine from Memories, Search & Replace). v4 microcopy
 * carries over verbatim; the deferred buttons get a "(not yet available)"
 * suffix on their tooltip (the list-screen precedent for disabled affordances).
 *
 * Group badges render as plain (non-navigable) chips — v5 has no `/groups`
 * route yet, so v4's `Link href="/aurora/groups/:id"` is intentionally
 * dropped rather than pointed at a dead route.
 */
@Component({
  selector: 'qt-character-header',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div
      class="mb-8 grid grid-cols-1 sm:grid-cols-[auto_1fr] items-stretch gap-6 rounded-2xl border qt-border-default/60 qt-bg-card/80 p-6 qt-shadow-sm md:grid-cols-[auto_1fr_auto]"
    >
      <!-- Avatar -->
      <div class="relative overflow-hidden rounded-lg w-24 h-30" style="aspect-ratio: 4/5">
        @if (avatarSrc()) {
          <img
            [src]="avatarSrc()"
            [alt]="character().name"
            class="absolute inset-0 h-full w-full object-cover"
          />
        } @else {
          <div class="absolute inset-0 flex items-center justify-center qt-bg-muted">
            <span class="text-3xl font-bold qt-text-secondary">{{ initial() }}</span>
          </div>
        }
      </div>

      <!-- Name / title / stats / groups -->
      <div class="flex min-w-0 flex-col">
        <div class="space-y-1">
          <div class="flex items-start justify-between gap-4">
            <h1 class="qt-heading-1 min-w-0 flex items-center gap-3">
              {{ character().name }}
              @if (isArchived()) {
                <span
                  class="inline-flex flex-shrink-0 items-center rounded-full border qt-border-default qt-bg-muted px-2.5 py-0.5 text-sm font-medium qt-text-secondary"
                  [title]="'Resting in the archive since ' + archivedSince()"
                >
                  Archived
                </span>
              }
            </h1>
            <!-- P4.D64 (v4 CharacterHeader.tsx:583): the toggle cluster is
                 hidden entirely for an archived character. -->
            @if (!isArchived()) {
            <div class="flex flex-shrink-0 items-center gap-2">
              <button
                type="button"
                class="text-2xl qt-text-favorite transition-transform hover:scale-110 disabled:opacity-50"
                [disabled]="togglingFavorite()"
                [title]="character().isFavorite ? 'Remove from favorites' : 'Add to favorites'"
                (click)="toggleFavorite.emit()"
              >
                {{ character().isFavorite ? '⭐' : '☆' }}
              </button>
              <button
                type="button"
                [class]="
                  'transition hover:scale-110 disabled:opacity-50 ' +
                  (character().canBeCarina ? 'qt-text-favorite' : 'qt-text-secondary')
                "
                [disabled]="togglingCarina()"
                [title]="
                  character().canBeCarina
                    ? 'Disable Carina answers (@-queries)'
                    : 'Enable Carina answers (@-queries)'
                "
                (click)="toggleCarina.emit()"
              >
                <qt-icon name="monitor" class="w-6 h-6" />
              </button>
              <button
                type="button"
                [class]="
                  'transition hover:scale-110 disabled:opacity-50 ' +
                  (character().controlledBy === 'user' ? 'qt-text-favorite' : 'qt-text-secondary')
                "
                [disabled]="togglingControlledBy()"
                [title]="
                  character().controlledBy === 'user'
                    ? 'Switch to LLM control'
                    : 'Switch to user control'
                "
                (click)="toggleControlledBy.emit()"
              >
                <qt-icon name="user" class="w-6 h-6" />
              </button>
            </div>
            }
          </div>
          @if (character().title || character().pronouns) {
            <div class="flex items-baseline justify-between gap-4">
              @if (character().title) {
                <h2 class="qt-heading-2 qt-text-secondary min-w-0">{{ character().title }}</h2>
              } @else {
                <span></span>
              }
              @if (character().pronouns; as pronouns) {
                <span
                  class="flex-shrink-0 cursor-help qt-text-small qt-text-secondary"
                  [title]="
                    'Pronouns used to refer to ' +
                    character().name +
                    ': ' +
                    pronouns.subject +
                    ' (subject) / ' +
                    pronouns.object +
                    ' (object) / ' +
                    pronouns.possessive +
                    ' (possessive).'
                  "
                >
                  {{ pronouns.subject }}/{{ pronouns.object }}/{{ pronouns.possessive }}
                </span>
              }
            </div>
          }
        </div>

        @if (character().aliases.length > 0) {
          <div class="mt-3 flex flex-wrap gap-1.5">
            @for (alias of character().aliases; track alias) {
              <span
                class="inline-flex items-center rounded-full border qt-border-default qt-bg-muted px-2 py-0.5 text-xs qt-text-secondary"
              >
                {{ alias }}
              </span>
            }
          </div>
        }

        <div class="mt-auto space-y-2 pt-4">
          @if (statItems().length > 0) {
            <div
              class="flex flex-wrap items-center gap-x-2 gap-y-1 qt-text-small qt-text-secondary"
            >
              @for (s of statItems(); track s.key; let i = $index) {
                @if (i > 0) {
                  <span class="opacity-40" aria-hidden="true">|</span>
                }
                <span class="cursor-help" [title]="s.tip">
                  <strong class="font-semibold text-foreground">{{ s.value }}</strong> {{ s.label }}
                </span>
              }
            </div>
          }
          @if (groups().length > 0) {
            <div class="flex flex-wrap items-center gap-1.5">
              @for (g of groups(); track g.id) {
                <span
                  class="inline-flex items-center gap-1.5 rounded-full border qt-border-default qt-bg-muted px-2 py-0.5 text-xs font-medium text-foreground"
                  [title]="g.description?.trim() || g.name"
                >
                  <span
                    class="flex h-4 w-4 items-center justify-center rounded-full text-[10px] leading-none"
                    [style.background-color]="g.color || 'var(--muted)'"
                  >
                    {{ g.icon || '' }}
                  </span>
                  <span>{{ g.name }}</span>
                </span>
              }
            </div>
          }
        </div>
      </div>

      <!-- Actions. P4.D64 forks the whole column (v4 CharacterHeader.tsx:599):
           an archived character gets ONE Rehydrate button in place of the live
           cluster — no chat, no conversion, no refinement. -->
      @if (isArchived()) {
      <div class="flex flex-shrink-0 flex-col gap-2">
        <button
          type="button"
          class="inline-flex items-center justify-center gap-1.5 rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground shadow hover:qt-bg-primary/90 disabled:opacity-50"
          [disabled]="rehydrating()"
          title="Restore this character from their archive bundle and wake them"
          (click)="rehydrate.emit()"
        >
          {{ rehydrating() ? 'Rehydrating…' : 'Rehydrate' }}
        </button>
      </div>
      } @else {
      <div class="flex flex-shrink-0 flex-col gap-2">
        <a
          [routerLink]="['/characters', character().id]"
          [queryParams]="{ action: 'chat' }"
          class="inline-flex items-center justify-center rounded-lg bg-success px-4 py-2 text-sm font-semibold qt-text-success-foreground qt-shadow-sm hover:qt-bg-success/90"
        >
          Start Chat
        </a>
        <button
          type="button"
          [disabled]="togglingNpc()"
          class="inline-flex items-center justify-center rounded-lg border qt-border-default qt-bg-card px-4 py-2 qt-label text-foreground qt-shadow-sm hover:qt-bg-muted disabled:opacity-50"
          (click)="toggleNpc.emit()"
        >
          {{
            togglingNpc()
              ? 'Converting...'
              : character().npc
                ? 'Convert to Character'
                : 'Convert to NPC'
          }}
        </button>
        <button
          type="button"
          disabled
          class="inline-flex items-center justify-center gap-1.5 rounded-lg border qt-border-default qt-bg-card px-4 py-2 qt-label text-foreground qt-shadow-sm disabled:opacity-50"
          title="Generate a standalone system prompt for use in external tools (not yet available)"
        >
          <qt-icon name="file" class="w-4 h-4" />
          Non-Quilltap Prompt
        </button>
        <button
          type="button"
          disabled
          class="inline-flex items-center justify-center gap-1.5 rounded-lg border qt-border-default qt-bg-card px-4 py-2 qt-label text-foreground qt-shadow-sm disabled:opacity-50"
          title="Analyze memories and suggest character refinements (not yet available)"
        >
          <qt-icon name="book" class="w-4 h-4" />
          Refine from Memories
        </button>
        <button
          type="button"
          disabled
          class="inline-flex items-center justify-center gap-1.5 rounded-lg border qt-border-default qt-bg-card px-4 py-2 qt-label text-foreground qt-shadow-sm disabled:opacity-50"
          title="Search &amp; Replace across all chats and memories for this character (not yet available)"
        >
          <qt-icon name="search" class="w-4 h-4" />
          Search &amp; Replace
        </button>
        <!-- Archive sits at the END of the live cluster, after v5's three
             disabled deferrals (v4 CharacterHeader.tsx:620). The classes are
             the d69287d9-FIXED ones: text-foreground, matching its siblings
             exactly — the original qt-text-secondary is the muted token
             disabled controls use, so the one enabled action that packs a
             character away read as unavailable. -->
        <button
          type="button"
          class="inline-flex items-center justify-center gap-1.5 rounded-lg border qt-border-default qt-bg-card px-4 py-2 qt-label text-foreground qt-shadow-sm hover:qt-bg-muted"
          title="Pack this character's effects into a sealed bundle and set them resting in the archive"
          (click)="archive.emit()"
        >
          <qt-icon name="folder" class="w-4 h-4" />
          Archive
        </button>
      </div>
      }
    </div>
  `,
})
export class CharacterHeader {
  readonly character = input.required<CharacterDetail>();
  readonly stats = input<CharacterStats | null>(null);
  readonly groups = input<CharacterGroupBadge[]>([]);

  readonly togglingFavorite = input(false);
  readonly togglingCarina = input(false);
  readonly togglingControlledBy = input(false);
  readonly togglingNpc = input(false);
  /** P4.D64: a rehydration is in flight — the button disables and relabels. */
  readonly rehydrating = input(false);

  readonly toggleFavorite = output<void>();
  readonly toggleCarina = output<void>();
  readonly toggleControlledBy = output<void>();
  readonly toggleNpc = output<void>();
  /** Opens the archive confirm dialog (live characters only, v4 `onArchive`). */
  readonly archive = output<void>();
  /** Attempts rehydration (archived characters only, v4 `onRehydrate`). */
  readonly rehydrate = output<void>();

  /** v4 `isArchived = Boolean(character?.archivedAt)`. */
  protected readonly isArchived = computed(() => Boolean(this.character().archivedAt));

  /**
   * The badge tooltip's date — v4's unguarded
   * `new Date(archivedAt).toLocaleDateString()`; visibility keys off
   * {@link isArchived} so a malformed stamp still shows the badge (the roster
   * card carries the same note).
   */
  protected readonly archivedSince = computed(() =>
    new Date(this.character().archivedAt ?? '').toLocaleDateString(),
  );

  protected readonly avatarSrc = computed(() => {
    const c = this.character();
    return characterAvatarSrc(c.defaultImage, c.defaultImageId) ?? c.avatarUrl ?? null;
  });
  protected readonly initial = computed(() => (this.character().name[0] ?? '?').toUpperCase());

  protected readonly statItems = computed<StatItem[]>(() => {
    const s = this.stats();
    if (!s) return [];
    return [
      {
        key: 'memories',
        value: s.memories,
        label: s.memories === 1 ? 'memory' : 'memories',
        tip: "Entries in this character's Commonplace Book — what they remember from past conversations.",
      },
      {
        key: 'conversations',
        value: s.conversations,
        label: s.conversations === 1 ? 'conversation' : 'conversations',
        tip: 'Chats this character has taken part in.',
      },
      {
        key: 'wardrobe',
        value: s.wardrobeItems,
        label: s.wardrobeItems === 1 ? 'wardrobe item' : 'wardrobe items',
        tip: "Garments and accessories in this character's wardrobe.",
      },
      {
        key: 'photos',
        value: s.photos,
        label: s.photos === 1 ? 'photo' : 'photos',
        tip: "Portraits saved in this character's gallery.",
      },
      {
        key: 'scenarios',
        value: s.scenarios,
        label: s.scenarios === 1 ? 'scenario' : 'scenarios',
        tip: 'Scene-setting openers written for this character, so a chat can begin in a particular situation.',
      },
      {
        key: 'knowledge',
        value: s.knowledge,
        label: 'knowledge',
        tip: "Documents in this character's vault Knowledge folder — reference material they can draw on during chats.",
      },
      {
        key: 'core',
        value: s.core,
        label: 'core',
        tip: "Documents in this character's vault Core folder — a foundational packet that is periodically re-offered to them.",
      },
      {
        key: 'characterFiles',
        value: `${s.characterFiles}/${s.characterFilesTotal}`,
        label: 'character files',
        tip: `How many of the ${s.characterFilesTotal} canonical managed vault files are present (identity, description, personality, and the like). ${s.characterFiles}/${s.characterFilesTotal} means the vault is complete; a shortfall means some are missing.`,
      },
    ];
  });
}
