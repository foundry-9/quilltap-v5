import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnInit,
  computed,
  effect,
  inject,
  input,
  signal,
  viewChild,
} from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import type {
  CharacterListItem,
  ProjectDetail,
  ProjectRosterCharacter,
} from '../../../core/core-contract';
import { QuickHideService } from '../../../quick-hide/quick-hide.service';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { Icon } from '../../../ui/icon';
import { Avatar } from '../../../ui/avatar';
import { ToastService } from '../../../ui/toast.service';
import {
  characterAvatarSrc,
  characterKeys,
  fetchCharacterList,
} from '../../characters/characters.api';
import {
  addProjectCharacter,
  projectKeys,
  removeProjectCharacter,
  updateProject,
} from '../projects.api';

/**
 * The project Characters card (v4 `app/prospero/[id]/components/
 * CharactersCard.tsx`, as `9753d0eb2` left it): an "Allow Any Character"
 * toggle (immediate PUT) over either an explainer (Allow Any ON) or the roster
 * — an Add-character picker and a 2-col avatar grid with a per-card remove and
 * avatars linking into the character view.
 *
 * v4: "The roster decides which characters may use their tools on the
 * project's files and its shared wardrobe (see `lib/projects/roster-access.ts`).
 * It does not decide who may chat in the project. When "Allow Any Character"
 * is off, the roster is curated by hand here — add from the picker, remove per
 * card."
 *
 * The roster is the GET's ENRICHED `characterRoster` (P4.D247 closed the
 * phantom `roster` key this card read from P4.6l on, which no server path ever
 * wrote — so no v5 project had shown its roster). The handlers live here, not
 * in the detail screen (P4.29's shape): each save awaits the detail
 * invalidation, i.e. a `projectGet` refetch, rather than swapping a PUT body in.
 */
@Component({
  selector: 'qt-project-characters-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, CollapsibleCard, Icon, Avatar],
  template: `
    <qt-collapsible-card
      title="Characters"
      [description]="subtitle()"
      icon="characters"
      [isOpen]="expanded()"
      (openChange)="expanded.set($event)"
    >
      <div class="flex items-center justify-between gap-3 px-2 py-3 qt-bg-muted rounded-lg mb-3">
        <div>
          <h4 class="qt-label text-foreground">Allow Any Character</h4>
          <p class="qt-text-xs qt-text-secondary">
            {{
              project().allowAnyCharacter
                ? 'Every character may use the project files and shared wardrobe.'
                : 'Only roster characters may use the project files and shared wardrobe.'
            }}
          </p>
        </div>
        <button
          type="button"
          class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors"
          [class.bg-primary]="project().allowAnyCharacter"
          [class.qt-bg-muted]="!project().allowAnyCharacter"
          role="switch"
          [attr.aria-checked]="project().allowAnyCharacter"
          aria-label="Allow Any Character"
          (click)="toggleAllowAny()"
        >
          <span
            class="inline-block h-4 w-4 transform rounded-full qt-bg-toggle-knob transition-transform"
            [class.translate-x-6]="project().allowAnyCharacter"
            [class.translate-x-1]="!project().allowAnyCharacter"
          ></span>
        </button>
      </div>

      @if (!rosterEditable()) {
        <div class="text-center qt-text-secondary py-2">
          <p class="qt-text-small">
            Any character in a project chat may read and edit its files and borrow from its
            wardrobe. Turn this off to choose who may.
          </p>
        </div>
      } @else {
        <div class="mb-3">
          @if (!pickerOpen()) {
            <button
              type="button"
              class="qt-button qt-button-secondary qt-button-sm w-full"
              (click)="pickerOpen.set(true)"
            >
              Add character
            </button>
          } @else {
            <div class="rounded-lg qt-border qt-bg-surface p-2">
              <div class="flex items-center gap-2">
                <input
                  #searchInput
                  type="text"
                  class="qt-input flex-1"
                  placeholder="Search characters…"
                  aria-label="Search characters to add"
                  [value]="search()"
                  (input)="search.set($any($event.target).value)"
                />
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm"
                  (click)="closePicker()"
                >
                  Done
                </button>
              </div>
              <div class="mt-2 max-h-56 overflow-y-auto">
                @if (charactersQuery.isLoading()) {
                  <p class="p-2 qt-text-small qt-text-secondary">Loading characters…</p>
                } @else if (candidates().length === 0) {
                  <p class="p-2 qt-text-small qt-text-secondary">
                    {{
                      search().trim()
                        ? 'No characters match.'
                        : 'Every character is already on the roster.'
                    }}
                  </p>
                } @else {
                  <ul>
                    @for (char of candidates(); track char.id) {
                      <li>
                        <button
                          type="button"
                          class="w-full flex items-center gap-2 p-2 rounded-md text-left hover:qt-bg-muted transition-colors disabled:opacity-50"
                          [disabled]="addingId() !== null"
                          (click)="addCharacter(char.id)"
                        >
                          <qt-avatar [name]="char.name" [src]="avatarSrc(char)" size="xs" />
                          <span class="flex-1 min-w-0">
                            <span class="block text-sm text-foreground truncate">{{
                              char.name
                            }}</span>
                            @if (char.title) {
                              <span class="block qt-text-xs qt-text-secondary truncate">{{
                                char.title
                              }}</span>
                            }
                          </span>
                          <span class="qt-text-xs qt-text-secondary">{{
                            addingId() === char.id ? 'Adding…' : 'Add'
                          }}</span>
                        </button>
                      </li>
                    }
                  </ul>
                }
              </div>
            </div>
          }
        </div>

        @if (roster().length === 0) {
          <div class="text-center qt-text-secondary py-2">
            <p>
              {{
                project().characterRoster.length === 0
                  ? 'No characters in the roster yet.'
                  : 'No visible characters (some may be hidden).'
              }}
            </p>
            @if (project().characterRoster.length === 0) {
              <p class="qt-text-small mt-1">
                Until someone is added, no character may use the project files or shared wardrobe.
              </p>
            }
          </div>
        } @else {
          <div class="max-h-80 overflow-y-auto">
            <div class="grid grid-cols-2 gap-2">
              @for (char of roster(); track char.id) {
                <div
                  class="relative flex flex-col p-3 rounded-lg qt-border qt-bg-surface hover:qt-border-primary hover:qt-shadow-md transition-all group"
                >
                  <button
                    type="button"
                    class="absolute top-1 right-1 p-1 rounded-full opacity-60 group-hover:opacity-100 focus:opacity-100 qt-text-secondary hover:qt-text-destructive hover:qt-bg-destructive/10 transition-all"
                    title="Remove from roster"
                    [attr.aria-label]="'Remove ' + (char.name || 'character') + ' from roster'"
                    (click)="removeCharacter(char.id)"
                  >
                    <qt-icon name="close" class="w-3.5 h-3.5" />
                  </button>
                  <a
                    [routerLink]="['/characters', char.id]"
                    class="flex flex-col items-center gap-2 hover:opacity-80 transition-opacity"
                  >
                    <qt-avatar [name]="char.name || 'Unknown'" [src]="avatarSrc(char)" size="md" />
                    <div class="text-center w-full">
                      <h4 class="text-sm font-semibold text-foreground truncate px-1">
                        {{ char.name || 'Unknown Character' }}
                      </h4>
                      <p class="qt-text-xs qt-text-secondary">{{ chatLabel(char) }}</p>
                    </div>
                  </a>
                </div>
              }
            </div>
          </div>
        }
      }
    </qt-collapsible-card>
  `,
})
export class ProjectCharactersCard implements OnInit {
  readonly project = input.required<ProjectDetail>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly quickHide = inject(QuickHideService);
  private readonly toasts = inject(ToastService);

  /**
   * v4's `expanded` prop. The collapsible card runs CONTROLLED so the picker
   * query below can carry v4's `expanded` conjunct: the card's own signals
   * (incl. `pickerOpen`) outlive a collapse, as React's card state does, so
   * without it a collapsed card with the picker left open would keep
   * refetching the list on every invalidation (survey §D3).
   */
  protected readonly expanded = signal(false);
  /** v4 `:57-59`. */
  protected readonly pickerOpen = signal(false);
  protected readonly search = signal('');
  protected readonly addingId = signal<string | null>(null);

  /** v4 `:61` — the roster is curated by hand only while Allow Any is OFF. */
  protected readonly rosterEditable = computed(() => !this.project().allowAnyCharacter);

  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');

  /**
   * v4 `:74-78` — every live (non-archived) character, fetched only while the
   * picker is open: the SHARED unfiltered list key and fetcher the five
   * dialogs and the mention source read (no second cache entry; survey §D7).
   * Archived characters are excluded server side by default.
   */
  protected readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(),
    queryFn: () => fetchCharacterList(this.core),
    enabled: this.expanded() && this.rosterEditable() && this.pickerOpen(),
  }));

  constructor() {
    // v4 `autoFocus` on the search box. React focuses it as it mounts; the
    // signal `viewChild` resolves on the render that creates it, so this effect
    // is the same moment (`wardrobe/outfit-quick-pick.ts`'s precedent — a bare
    // `autofocus` attribute does nothing on a node inserted after load).
    effect(() => {
      this.searchInput()?.nativeElement.focus();
    });
  }

  ngOnInit(): void {
    // Bound inputs have resolved by ngOnInit — the uncontrolled card seeded
    // its open state from `defaultOpen` at the same moment.
    this.expanded.set(this.defaultOpen());
  }

  /**
   * v4 `:64-71`: dedupe by id (a character can appear twice in the roster),
   * then drop any carrying a hidden tag. Both the grid and the "{n} characters
   * in roster" subtitle read this filtered list, exactly as v4's
   * `visibleCharacters` feeds its own count.
   */
  protected readonly roster = computed<ProjectRosterCharacter[]>(() => {
    const seen = new Set<string>();
    return this.project().characterRoster.filter((char) => {
      if (!char.id || seen.has(char.id)) return false;
      seen.add(char.id);
      return !this.quickHide.shouldHideByIds(char.tags ?? []);
    });
  });

  /**
   * v4 `:80-88`. `onRoster` is built from the RAW roster (pre-dedupe,
   * pre-quick-hide), so a hidden roster member is never offered again; the
   * search matches name OR title; the order is the JS default-locale
   * `localeCompare` (never routed through any other collator — survey §D6).
   */
  protected readonly candidates = computed<CharacterListItem[]>(() => {
    const onRoster = new Set(this.project().characterRoster.map((c) => c.id));
    const term = this.search().trim().toLowerCase();
    return (this.charactersQuery.data() ?? [])
      .filter((c) => !onRoster.has(c.id))
      .filter((c) => !this.quickHide.shouldHideByIds(c.tags ?? []))
      .filter(
        (c) =>
          !term ||
          c.name.toLowerCase().includes(term) ||
          (c.title ?? '').toLowerCase().includes(term),
      )
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  /** v4 `:116-118`. */
  protected readonly subtitle = computed(() => {
    if (this.project().allowAnyCharacter) return 'Open to every character';
    const n = this.roster().length;
    return `${n} character${n !== 1 ? 's' : ''} in roster`;
  });

  protected avatarSrc(char: ProjectRosterCharacter | CharacterListItem): string | null {
    return characterAvatarSrc(char.defaultImage, char.defaultImageId);
  }

  protected chatLabel(char: ProjectRosterCharacter): string {
    const n = char.chatCount ?? 0;
    return `${n} chat${n !== 1 ? 's' : ''}`;
  }

  /** v4 `:99-102` — closing the picker clears the search too. */
  protected closePicker(): void {
    this.pickerOpen.set(false);
    this.search.set('');
  }

  /**
   * v4 `useProjectDetail.ts:88-108` — toast only, no inline surface. A refusal
   * is v4's FIXED `Failed to update project` (v4 throws it on a non-OK
   * response and never reads the body); a thrown `Error` (v4's fetch reject)
   * its own message; anything else the catch's fallback.
   */
  protected async toggleAllowAny(): Promise<void> {
    try {
      const updated = await updateProject(this.core, this.project().id, {
        allowAnyCharacter: !this.project().allowAnyCharacter,
      });
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
      this.toasts.showSuccess(
        updated.allowAnyCharacter
          ? 'Every character may now use the project files and wardrobe'
          : 'Only roster characters may use the project files and wardrobe',
      );
    } catch (err) {
      this.toasts.showError(
        err instanceof CoreDispatchError
          ? 'Failed to update project'
          : err instanceof Error
            ? err.message
            : 'Failed to update setting',
      );
    }
  }

  /**
   * v4 `CharactersCard.tsx:90-97` + `useProjectDetail.ts:271-290`. The detail
   * refetch is awaited BEFORE the toast and before `addingId` clears (the
   * card's `finally` runs after the hook resolves), so the added character has
   * already left the candidates when the rows re-enable. A refusal toasts the
   * server's own sentence (v4 `data?.error || 'Failed to add character'`). The
   * picker stays open.
   */
  protected async addCharacter(characterId: string): Promise<void> {
    this.addingId.set(characterId);
    try {
      await addProjectCharacter(this.core, this.project().id, characterId);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
      this.toasts.showSuccess('Character added to the roster');
    } catch (err) {
      this.toasts.showError(
        err instanceof CoreDispatchError
          ? err.message || 'Failed to add character'
          : err instanceof Error
            ? err.message
            : 'Failed to add character',
      );
    } finally {
      this.addingId.set(null);
    }
  }

  /**
   * v4 `useProjectDetail.ts:292-308` — toast only, no inline surface; a refusal
   * is v4's FIXED `Failed to remove character` (the body is never read).
   */
  protected async removeCharacter(characterId: string): Promise<void> {
    try {
      await removeProjectCharacter(this.core, this.project().id, characterId);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
      this.toasts.showSuccess('Character removed from the roster');
    } catch (err) {
      this.toasts.showError(
        err instanceof CoreDispatchError
          ? 'Failed to remove character'
          : err instanceof Error
            ? err.message
            : 'Failed to remove character',
      );
    }
  }
}
