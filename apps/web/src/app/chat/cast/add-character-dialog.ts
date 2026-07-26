import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { addParticipant } from '../chat-cast.api';
import { CoreClient } from '../../core/core-client';
import type {
  CharacterListItem,
  ChatCreateOutfitSelectionInput,
  ConnectionProfileDto,
} from '../../core/core-contract';
import { MarkdownField } from '../../editor/markdown-field';
import { characterAvatarSrc, characterKeys, fetchCharacterList } from '../../screens/characters/characters.api';
import { OutfitSelector, type OutfitSelectorCharacter } from '../../screens/new-chat/outfit-selector';
import { Avatar } from '../../ui/avatar';
import { Icon } from '../../ui/icon';
import { ProviderModelBadge } from '../sidebar/provider-model-badge';
import { CreateNpcDialog } from './create-npc-dialog';

/**
 * v4's sentinel for "the human types for this character" in the Controlled-By
 * select (`AddCharacterDialog.tsx:60`). It is a select VALUE, never a wire
 * value: choosing it sends `controlledBy: 'user'` and omits
 * `connectionProfileId` entirely (v4 `:196-199` — "schema doesn't accept null").
 */
export const USER_IMPERSONATION_VALUE = '__user_impersonation__';

/**
 * Add Character (v4 `components/chat/AddCharacterDialog.tsx`, 631 LOC): the
 * searchable roster of characters NOT already in the chat, split into regular
 * characters and NPCs, plus the connection-profile ("Controlled By") select,
 * the history-access checkbox, the join scenario, and the starting outfit.
 * Committing posts §1 `chatAddParticipant` (v4 `?action=add-participant`).
 *
 * ## What is ported exactly, and why it matters
 *
 * - **The default-profile fallback chain** (v4 `:118-136`): the character's own
 *   default, but only if that profile still exists in the user's list; else the
 *   user's default; else the first profile; else nothing. The existence check is
 *   the subtle part — a character pointing at a deleted profile must fall
 *   through, not select a phantom.
 * - **The roster split and sort** (`:138-160`): characters already in the chat
 *   are removed, the search matches name OR title case-insensitively, and each
 *   group is sorted by `localeCompare`. NPCs come after a labelled divider.
 * - **The body's conditional keys** (`:186-209`), which live in
 *   {@link addParticipant}: no `connectionProfileId` when the seat is the
 *   human's, `joinScenario` only when non-blank (trimmed), `outfitSelection`
 *   only when the selector produced one — the server then defaults the outfit to
 *   `mode: 'default'`, matching the new-chat flow.
 * - **The nested dialogs' click-outside suppression** (`:161-164`): while Create
 *   NPC is open, a backdrop click or Escape must not close THIS dialog too.
 *
 * ## Deferred, loudly and by name
 *
 * **Summon from Lore** (v4 `SummonFromLoreModal.tsx`, nested at
 * `AddCharacterDialog.tsx:624`) is not ported. Its 84 lines are a wrapper around
 * `components/settings/ai-import/AIImportWizard` (703 LOC, unported) — its own
 * header says it "deliberately reuses AIImportWizard and the import-execute path
 * rather than forking either", so porting Summon means porting the Aurora
 * AI-import wizard, a round of its own. v4's entry point is KEPT and says so
 * when pressed, rather than being quietly dropped from the grid.
 */
@Component({
  selector: 'qt-add-character-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon, MarkdownField, OutfitSelector, ProviderModelBadge, CreateNpcDialog],
  template: `
    <div class="qt-dialog-overlay p-4" (click)="onBackdrop($event)">
      <div
        class="qt-dialog max-w-2xl max-h-[90vh] flex flex-col"
        role="dialog"
        aria-modal="true"
        (click)="$event.stopPropagation()"
      >
        <div class="qt-dialog-header flex items-center justify-between">
          <h2 class="qt-dialog-title">Add Character to Chat</h2>
          <button
            type="button"
            class="qt-button qt-button-ghost p-2"
            aria-label="Close"
            [disabled]="adding()"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-6 h-6" />
          </button>
        </div>

        <div class="qt-dialog-body flex-1 overflow-y-auto">
          @if (error(); as message) {
            <div class="qt-alert-error mb-4" role="alert">{{ message }}</div>
          }

          @if (charactersQuery.isPending()) {
            <div class="flex items-center justify-center py-12 qt-text-small">
              Loading characters...
            </div>
          } @else {
            <div class="space-y-6">
              <!-- Character selection (v4 :299-483) -->
              <div>
                <label class="block text-sm qt-text-primary mb-2" for="qt-add-character-search">
                  Select Character
                </label>
                <input
                  id="qt-add-character-search"
                  type="text"
                  class="qt-input mb-3"
                  placeholder="Search characters..."
                  aria-label="Search characters"
                  [value]="search()"
                  [disabled]="adding()"
                  (input)="search.set($any($event.target).value)"
                />

                <div class="grid grid-cols-2 sm:grid-cols-3 gap-3 max-h-64 overflow-y-auto p-1">
                  @if (regularCharacters().length === 0 && npcCharacters().length === 0) {
                    <div class="col-span-full text-center py-8 qt-text-secondary">
                      {{
                        search()
                          ? 'No matching characters found'
                          : 'All your characters are already in this chat'
                      }}
                    </div>
                  } @else {
                    @for (character of regularCharacters(); track character.id) {
                      <button
                        type="button"
                        [class]="rosterButtonClass(character.id)"
                        [disabled]="adding()"
                        (click)="selectedCharacterId.set(character.id)"
                      >
                        <div class="flex items-center gap-3">
                          <qt-avatar
                            [name]="character.name"
                            [src]="avatarSrc(character)"
                            size="md"
                          />
                          <div class="min-w-0 flex-1">
                            <div class="font-semibold text-foreground truncate">
                              {{ character.name }}
                            </div>
                            @if (character.title) {
                              <div class="qt-text-xs italic truncate">{{ character.title }}</div>
                            }
                          </div>
                          @if (selectedCharacterId() === character.id) {
                            <qt-icon
                              name="check-circle"
                              class="w-5 h-5 text-primary flex-shrink-0"
                            />
                          }
                        </div>
                      </button>
                    }

                    @if (npcCharacters().length > 0) {
                      <div class="col-span-full flex items-center gap-2 my-2">
                        <div class="flex-1 border-t qt-border-default"></div>
                        <span class="qt-text-xs qt-text-secondary font-medium">NPCs</span>
                        <div class="flex-1 border-t qt-border-default"></div>
                      </div>
                      @for (character of npcCharacters(); track character.id) {
                        <button
                          type="button"
                          [class]="rosterButtonClass(character.id)"
                          [disabled]="adding()"
                          (click)="selectedCharacterId.set(character.id)"
                        >
                          <div class="flex items-center gap-3">
                            <qt-avatar
                              [name]="character.name"
                              [src]="avatarSrc(character)"
                              size="md"
                            />
                            <div class="min-w-0 flex-1">
                              <div class="font-semibold text-foreground truncate">
                                {{ character.name }}
                              </div>
                              @if (character.title) {
                                <div class="qt-text-xs italic truncate">{{ character.title }}</div>
                              }
                            </div>
                            @if (selectedCharacterId() === character.id) {
                              <qt-icon
                                name="check-circle"
                                class="w-5 h-5 text-primary flex-shrink-0"
                              />
                            }
                          </div>
                        </button>
                      }
                    }

                    <!-- Create New NPC (v4 :434-455) -->
                    <button
                      type="button"
                      class="p-3 rounded-lg border border-dashed qt-border-default hover:qt-border-primary/50 hover:qt-bg-primary/5 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                      [disabled]="adding()"
                      (click)="createNpcOpen.set(true)"
                    >
                      <div class="flex items-center gap-3">
                        <div
                          class="w-10 h-10 flex items-center justify-center rounded qt-bg-primary/10 text-primary flex-shrink-0"
                        >
                          <qt-icon name="user-plus" class="w-6 h-6" />
                        </div>
                        <div class="min-w-0 flex-1 text-left">
                          <div class="font-semibold text-primary truncate">Create New NPC</div>
                          <div class="qt-text-xs qt-text-secondary truncate">
                            Add an ad-hoc character
                          </div>
                        </div>
                      </div>
                    </button>

                    <!-- Summon from Lore — the entry point stays; pressing it
                         says what it is waiting on (v4 :457-479). -->
                    <button
                      type="button"
                      class="p-3 rounded-lg border border-dashed qt-border-default hover:qt-border-primary/50 hover:qt-bg-primary/5 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                      [disabled]="adding()"
                      title="Not yet available: Summon from Lore needs Aurora’s AI-import wizard"
                      (click)="summonRefused.set(true)"
                    >
                      <div class="flex items-center gap-3">
                        <div
                          class="w-10 h-10 flex items-center justify-center rounded qt-bg-primary/10 text-primary flex-shrink-0"
                        >
                          <qt-icon name="sparkles" class="w-6 h-6" />
                        </div>
                        <div class="min-w-0 flex-1 text-left">
                          <div class="font-semibold text-primary truncate">Summon from Lore</div>
                          <div class="qt-text-xs qt-text-secondary truncate">
                            Conjure a soul from your worldbuilding notes
                          </div>
                        </div>
                      </div>
                    </button>

                    @if (summonRefused()) {
                      <div class="col-span-full qt-alert-warning" role="status">
                        Summoning from lore is not yet available in this edition: the séance
                        depends on Aurora&rsquo;s import wizard, which has not made the crossing.
                        (v4 <code>SummonFromLoreModal.tsx</code>, over
                        <code>components/settings/ai-import/AIImportWizard</code>.)
                      </div>
                    }
                  }
                </div>
              </div>

              @if (selectedCharacterId()) {
                <!-- Controlled By (v4 :485-521) -->
                <div>
                  <label class="block text-sm qt-text-primary mb-2" for="qt-add-character-profile">
                    Controlled By
                  </label>
                  <select
                    id="qt-add-character-profile"
                    class="qt-select"
                    [disabled]="adding()"
                    [value]="selectedProfileValue() ?? ''"
                    (change)="onProfileChange($any($event.target).value)"
                  >
                    <option value="">Select who controls this character...</option>
                    <option [value]="USER_IMPERSONATION_VALUE">
                      User (you will type for this character)
                    </option>
                    <optgroup label="LLM Backends">
                      @for (profile of profiles(); track profile.id) {
                        <option
                          [value]="profile.id"
                          [selected]="profile.id === selectedProfileValue()"
                        >
                          {{ profile.name }} ({{ profile.provider }}: {{ profile.modelName }})
                        </option>
                      }
                    </optgroup>
                  </select>
                  @if (selectedProfile(); as profile) {
                    <div class="mt-1">
                      <qt-provider-model-badge
                        [provider]="profile.provider"
                        [modelName]="profile.modelName"
                        size="sm"
                      />
                    </div>
                  }
                  @if (profiles().length === 0) {
                    <p class="text-sm qt-text-warning mt-1">
                      No connection profiles available. Create one in Settings to use LLM control.
                    </p>
                  }
                </div>

                <!-- History access (v4 :523-543) -->
                <div class="flex items-start gap-3">
                  <input
                    type="checkbox"
                    id="hasHistoryAccess"
                    class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-ring"
                    [checked]="hasHistoryAccess()"
                    [disabled]="adding()"
                    (change)="hasHistoryAccess.set($any($event.target).checked)"
                  />
                  <div>
                    <label for="hasHistoryAccess" class="text-sm qt-text-primary cursor-pointer">
                      Include chat history in context
                    </label>
                    <p class="qt-text-xs mt-0.5">
                      If checked, this character will see messages from before they joined. If
                      unchecked, they will only see messages from their join point onward.
                    </p>
                  </div>
                </div>

                <!-- Join scenario (v4 :545-566) -->
                <div>
                  <label class="block text-sm qt-text-primary mb-2">
                    How did they join?
                    <span class="qt-text-secondary font-normal">(optional)</span>
                  </label>
                  <p class="qt-text-xs mb-2">
                    Included in the character&rsquo;s context to explain how they joined the
                    conversation.
                  </p>
                  <qt-markdown-field
                    [value]="joinScenario()"
                    [disabled]="adding()"
                    [recordKey]="selectedCharacterId()"
                    ariaLabel="Join scenario"
                    minHeight="6rem"
                    (contentChange)="joinScenario.set($event)"
                  />
                </div>

                <!-- Starting outfit (v4 :568-577) -->
                <qt-outfit-selector
                  [characters]="outfitCharacters()"
                  [disabled]="adding()"
                  (selectionsChange)="onOutfitSelections($event)"
                />
              }
            </div>
          }
        </div>

        <div class="qt-dialog-footer flex items-center justify-end gap-3">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="adding()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="adding() || !selectedCharacterId() || !selectedProfileValue()"
            (click)="commit()"
          >
            @if (adding()) {
              Adding...
            } @else {
              <qt-icon name="plus" class="w-4 h-4" />
              Add Character
            }
          </button>
        </div>
      </div>

      @if (createNpcOpen()) {
        <qt-create-npc-dialog
          (close)="createNpcOpen.set(false)"
          (created)="onNpcCreated($event)"
        />
      }
    </div>
  `,
})
export class AddCharacterDialog {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  /** Characters already in the chat — v4 filters them out of the roster. */
  readonly existingCharacterIds = input<string[]>([]);

  readonly close = output<void>();
  /** The character joined — v4 `onCharacterAdded()`, which refetches the chat. */
  readonly added = output<{ characterId: string; name: string }>();

  protected readonly USER_IMPERSONATION_VALUE = USER_IMPERSONATION_VALUE;

  protected readonly search = signal('');
  protected readonly selectedCharacterId = signal<string | null>(null);
  /** A profile id, {@link USER_IMPERSONATION_VALUE}, or null. */
  protected readonly selectedProfileValue = signal<string | null>(null);
  protected readonly hasHistoryAccess = signal(false);
  protected readonly joinScenario = signal('');
  protected readonly outfitSelection = signal<ChatCreateOutfitSelectionInput | null>(null);
  protected readonly adding = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly createNpcOpen = signal(false);
  protected readonly summonRefused = signal(false);

  protected readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(),
    queryFn: () => fetchCharacterList(this.core),
  }));

  protected readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'] as const,
    queryFn: async () => {
      const data = await this.core.dispatchData({ type: 'connectionProfileList' });
      return (data['profiles'] as ConnectionProfileDto[]) ?? [];
    },
  }));

  protected readonly characters = computed<CharacterListItem[]>(
    () => this.charactersQuery.data() ?? [],
  );
  protected readonly profiles = computed<ConnectionProfileDto[]>(
    () => this.profilesQuery.data() ?? [],
  );

  /**
   * v4 `:118-136` — seed the profile when a character is picked: its own default
   * (only if that profile STILL EXISTS), else the user's default, else the first.
   * A character pointing at a deleted profile falls through rather than
   * selecting a phantom.
   */
  private readonly seedProfile = effect(() => {
    const characterId = this.selectedCharacterId();
    if (!characterId) return;
    const profiles = this.profiles();
    const character = this.characters().find((c) => c.id === characterId);
    const characterDefault = character?.defaultConnectionProfileId ?? null;
    const characterDefaultExists = characterDefault
      ? profiles.some((p) => p.id === characterDefault)
      : false;
    const resolved = characterDefaultExists
      ? characterDefault
      : (profiles.find((p) => p.isDefault)?.id ?? profiles[0]?.id ?? null);
    if (resolved) {
      this.selectedProfileValue.set(resolved);
    }
  });

  /** The roster minus the current cast, matched on name OR title (v4 `:138-148`). */
  private readonly filteredCharacters = computed(() => {
    const existing = new Set(this.existingCharacterIds());
    const term = this.search().toLowerCase();
    return this.characters()
      .filter((c) => !existing.has(c.id))
      .filter(
        (c) =>
          term === '' ||
          c.name.toLowerCase().includes(term) ||
          (c.title ? c.title.toLowerCase().includes(term) : false),
      );
  });

  protected readonly regularCharacters = computed(() =>
    this.filteredCharacters()
      .filter((c) => c.npc !== true)
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  protected readonly npcCharacters = computed(() =>
    this.filteredCharacters()
      .filter((c) => c.npc === true)
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  private readonly selectedCharacter = computed(
    () => this.characters().find((c) => c.id === this.selectedCharacterId()) ?? null,
  );

  /** The badge under the select — only for a real LLM profile (v4 `:511-519`). */
  protected readonly selectedProfile = computed(() => {
    const value = this.selectedProfileValue();
    if (!value || value === USER_IMPERSONATION_VALUE) return null;
    return this.profiles().find((p) => p.id === value) ?? null;
  });

  /** v4 `outfitCharacters` (`:241-249`) — the one selected character. */
  protected readonly outfitCharacters = computed<OutfitSelectorCharacter[]>(() => {
    const character = this.selectedCharacter();
    if (!character) return [];
    return [
      {
        id: character.id,
        name: character.name,
        isUserControlled: this.selectedProfileValue() === USER_IMPERSONATION_VALUE,
        canChooseOutfit: character.canChooseOutfit ?? false,
      },
    ];
  });

  protected avatarSrc(character: CharacterListItem): string | null {
    return characterAvatarSrc(character.defaultImage, character.defaultImageId);
  }

  protected rosterButtonClass(characterId: string): string {
    const selected = this.selectedCharacterId() === characterId;
    return (
      'p-3 rounded-lg border text-left transition-all disabled:opacity-50 disabled:cursor-not-allowed ' +
      (selected
        ? 'qt-border-primary qt-bg-primary/10 ring-2 ring-primary'
        : 'qt-border-default hover:qt-border-primary/50 hover:qt-bg-muted/50')
    );
  }

  protected onProfileChange(value: string): void {
    this.selectedProfileValue.set(value || null);
  }

  /** v4 `handleOutfitSelectionsChange` — the selector's first (only) entry. */
  protected onOutfitSelections(selections: ChatCreateOutfitSelectionInput[]): void {
    this.outfitSelection.set(selections[0] ?? null);
  }

  /**
   * v4 `handleNPCCreated` (`:250-256`): preselect the new NPC and close the
   * nested dialog, leaving the operator to finish with the usual controls.
   */
  protected onNpcCreated(characterId: string): void {
    this.selectedCharacterId.set(characterId);
    this.createNpcOpen.set(false);
    void this.charactersQuery.refetch();
  }

  /**
   * Backdrop dismiss. v4 disables click-outside while the NPC dialog is open
   * (`useClickOutside(..., {enabled: isOpen && !isCreateNPCOpen})`, `:161-164`)
   * and while a request is in flight.
   */
  protected onBackdrop(event: Event): void {
    if (event.target !== event.currentTarget) return;
    if (this.adding() || this.createNpcOpen()) return;
    this.close.emit();
  }

  /** v4 `handleAddCharacter` (`:167-235`). */
  protected async commit(): Promise<void> {
    const chatId = this.chatId();
    const characterId = this.selectedCharacterId();
    const profileValue = this.selectedProfileValue();
    if (!characterId || !profileValue) {
      this.error.set('Please select a character and connection profile');
      return;
    }
    const isUser = profileValue === USER_IMPERSONATION_VALUE;
    const scenario = this.joinScenario().trim();
    const outfit = this.outfitSelection();

    this.adding.set(true);
    this.error.set(null);
    try {
      await addParticipant(this.core, chatId, {
        characterId,
        controlledBy: isUser ? 'user' : 'llm',
        hasHistoryAccess: this.hasHistoryAccess(),
        // v4 sends the profile ONLY for LLM control — the schema takes no null.
        ...(isUser ? {} : { connectionProfileId: profileValue }),
        // v4 sends the key only when the trimmed text is non-empty.
        ...(scenario ? { joinScenario: scenario } : {}),
        // Absent → the server applies the character's default wardrobe.
        ...(outfit ? { outfitSelection: outfit } : {}),
      });
      this.added.emit({
        characterId,
        name: this.selectedCharacter()?.name ?? 'Character',
      });
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to add character');
    } finally {
      this.adding.set(false);
    }
  }
}
