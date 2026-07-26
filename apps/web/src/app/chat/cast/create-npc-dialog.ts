import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { ConnectionProfileDto } from '../../core/core-contract';
import { MarkdownField } from '../../editor/markdown-field';
import { uploadCharacterPhoto } from '../../screens/characters/characters.api';
import { Icon } from '../../ui/icon';

/**
 * Create Ad-hoc NPC (v4 `components/chat/CreateNPCDialog.tsx`, 482 LOC), nested
 * inside the Add Character picker (`AddCharacterDialog.tsx:616`): a short form
 * — name, description, physical description, scenario, system prompt,
 * connection profile, optional avatar — that creates a character flagged `npc`
 * and hands its id back to the picker, preselected.
 *
 * ## v4's schema-shaped bodies, carried verbatim
 *
 * v4 learned three lessons the hard way and wrote them into `:161-203`; a port
 * that "tidies" them silently drops fields, because Zod strips unknown keys
 * rather than complaining:
 *
 *  - `personality` is a COPY of `description` (`:167`) — the NPC form collects
 *    one field and fills both vantage points with it.
 *  - the scenario is a one-element `scenarios[]` of `{id, title, content}`, NOT
 *    a scalar `scenario`.
 *  - the physical description is a singular `physicalDescription` OBJECT, not a
 *    plural array.
 *
 * ## The avatar sequence diverges, deliberately
 *
 * v4 uploads the file FIRST to `POST /api/v1/images` with
 * `referenceId: 'pending'`, creates the character, then pins the returned image
 * id with `?action=avatar` (`:141-243`). v5 has no generic images-upload route —
 * the ported upload leg is the character's own photo gallery
 * (`POST /api/v1/characters/{id}/photos`, P4.6m), which needs the character to
 * exist. So v5 creates the character first, uploads into its gallery, and pins
 * the returned `linkId` with `characterAvatar` — the same end state by the only
 * road v5 has. v4's tolerance is kept exactly: **an avatar failure does not fail
 * the creation** (`:236-241` logs and carries on), so the NPC still joins.
 */
@Component({
  selector: 'qt-create-npc-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MarkdownField],
  template: `
    <div class="qt-dialog-overlay p-4" (click)="onBackdrop($event)">
      <div
        class="qt-dialog max-w-2xl max-h-[90vh] flex flex-col"
        role="dialog"
        aria-modal="true"
        (click)="$event.stopPropagation()"
      >
        <div class="qt-dialog-header flex items-center justify-between">
          <h2 class="qt-dialog-title">Create Ad-hoc NPC</h2>
          <button
            type="button"
            class="qt-button qt-button-ghost p-2"
            aria-label="Close"
            [disabled]="creating()"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-6 h-6" />
          </button>
        </div>

        <div class="qt-dialog-body flex-1 overflow-y-auto">
          @if (error(); as message) {
            <div class="qt-alert-error mb-4" role="alert">{{ message }}</div>
          }

          <div class="space-y-4">
            <div>
              <label for="npc-name" class="block text-sm qt-text-primary mb-2">
                Name <span class="qt-text-destructive">*</span>
              </label>
              <input
                id="npc-name"
                type="text"
                class="qt-input"
                placeholder="Enter NPC name"
                [value]="name()"
                [disabled]="creating()"
                (input)="name.set($any($event.target).value)"
              />
            </div>

            <div>
              <label class="block text-sm qt-text-primary mb-2">
                Description <span class="qt-text-destructive">*</span>
              </label>
              <p class="qt-text-xs mb-2">
                Describe the NPC&rsquo;s personality and characteristics. Used as both description
                and personality.
              </p>
              <qt-markdown-field
                [value]="description()"
                [disabled]="creating()"
                ariaLabel="NPC description"
                minHeight="8rem"
                (contentChange)="description.set($event)"
              />
            </div>

            <div>
              <label class="block text-sm qt-text-primary mb-2">
                Physical Description
                <span class="qt-text-secondary font-normal">(optional)</span>
              </label>
              <p class="qt-text-xs mb-2">Describe the NPC&rsquo;s physical appearance.</p>
              <qt-markdown-field
                [value]="physicalDescription()"
                [disabled]="creating()"
                ariaLabel="Physical description"
                minHeight="6rem"
                (contentChange)="physicalDescription.set($event)"
              />
            </div>

            <div>
              <label class="block text-sm qt-text-primary mb-2">
                Scenario <span class="qt-text-secondary font-normal">(optional)</span>
              </label>
              <p class="qt-text-xs mb-2">Describe the scenario or context for this NPC.</p>
              <qt-markdown-field
                [value]="scenario()"
                [disabled]="creating()"
                ariaLabel="Scenario"
                minHeight="6rem"
                (contentChange)="scenario.set($event)"
              />
            </div>

            <div>
              <label class="block text-sm qt-text-primary mb-2">
                System Prompt <span class="qt-text-secondary font-normal">(optional)</span>
              </label>
              <p class="qt-text-xs mb-2">Custom system prompt for this NPC.</p>
              <qt-markdown-field
                [value]="systemPrompt()"
                [disabled]="creating()"
                ariaLabel="System prompt"
                minHeight="6rem"
                (contentChange)="systemPrompt.set($event)"
              />
            </div>

            <div>
              <label for="npc-profile" class="block text-sm qt-text-primary mb-2">
                Connection Profile <span class="qt-text-destructive">*</span>
              </label>
              <select
                id="npc-profile"
                class="qt-select"
                [disabled]="creating()"
                [value]="connectionProfileId() ?? ''"
                (change)="connectionProfileId.set($any($event.target).value || null)"
              >
                <option value="">Select a connection profile...</option>
                @for (profile of profiles(); track profile.id) {
                  <option [value]="profile.id" [selected]="profile.id === connectionProfileId()">
                    {{ profile.name }} ({{ profile.provider }}: {{ profile.modelName }})
                  </option>
                }
              </select>
              @if (profiles().length === 0) {
                <p class="text-sm qt-text-warning mt-1">
                  No connection profiles available. Please create one in Settings.
                </p>
              }
            </div>

            <div>
              <label class="block text-sm qt-text-primary mb-2">
                Avatar <span class="qt-text-secondary font-normal">(optional)</span>
              </label>
              <div class="flex items-center gap-3">
                <button
                  type="button"
                  class="qt-button qt-button-secondary"
                  [disabled]="creating()"
                  (click)="avatarInput.click()"
                >
                  <qt-icon name="image" class="w-4 h-4 mr-2" />
                  Choose Image
                </button>
                <input
                  #avatarInput
                  type="file"
                  accept="image/*"
                  class="hidden"
                  aria-label="NPC avatar"
                  [disabled]="creating()"
                  (change)="onAvatarPicked($event)"
                />
                @if (avatarFile(); as file) {
                  <span class="text-sm qt-text-success flex items-center gap-2">
                    <qt-icon name="check" class="w-4 h-4" />
                    {{ file.name }}
                  </span>
                } @else {
                  <span class="qt-text-muted text-sm">No image selected</span>
                }
              </div>
            </div>
          </div>
        </div>

        <div class="qt-dialog-footer flex items-center justify-end gap-3">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="creating()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="!canCreate()"
            (click)="commit()"
          >
            @if (creating()) {
              Creating...
            } @else {
              <qt-icon name="plus" class="w-4 h-4" />
              Create NPC
            }
          </button>
        </div>
      </div>
    </div>
  `,
})
export class CreateNpcDialog {
  private readonly core = inject(CoreClient);

  readonly close = output<void>();
  /** The new character's id — v4 `onNPCCreated(characterId)`. */
  readonly created = output<string>();

  protected readonly name = signal('');
  protected readonly description = signal('');
  protected readonly physicalDescription = signal('');
  protected readonly scenario = signal('');
  protected readonly systemPrompt = signal('');
  protected readonly connectionProfileId = signal<string | null>(null);
  protected readonly avatarFile = signal<File | null>(null);
  protected readonly creating = signal(false);
  protected readonly error = signal<string | null>(null);

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'] as const,
    queryFn: async () => {
      const data = await this.core.dispatchData({ type: 'connectionProfileList' });
      return (data['profiles'] as ConnectionProfileDto[]) ?? [];
    },
  }));

  protected readonly profiles = computed<ConnectionProfileDto[]>(
    () => this.profilesQuery.data() ?? [],
  );

  /** v4 `:60-67` — auto-select the first profile once the list arrives. */
  private readonly seedProfile = effect(() => {
    const profiles = this.profiles();
    if (profiles.length > 0 && !this.connectionProfileId()) {
      this.connectionProfileId.set(profiles[0]!.id);
    }
  });

  /** v4 disables Create until name, description and a profile are all present. */
  protected readonly canCreate = computed(
    () =>
      !this.creating() &&
      this.name().trim().length > 0 &&
      this.description().trim().length > 0 &&
      !!this.connectionProfileId(),
  );

  protected onBackdrop(event: Event): void {
    if (event.target !== event.currentTarget) return;
    if (this.creating()) return;
    this.close.emit();
  }

  /** v4 `handleAvatarFileChange` (`:107-118`) — image files only. */
  protected onAvatarPicked(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      this.error.set('Please select an image file');
      return;
    }
    this.error.set(null);
    this.avatarFile.set(file);
  }

  /** v4 `handleCreateNPC` (`:120-247`), resequenced for v5's upload leg. */
  protected async commit(): Promise<void> {
    const name = this.name().trim();
    const description = this.description().trim();
    const profileId = this.connectionProfileId();
    if (!name) {
      this.error.set('Please enter a name for the NPC');
      return;
    }
    if (!description) {
      this.error.set('Please enter a description for the NPC');
      return;
    }
    if (!profileId) {
      this.error.set('Please select a connection profile');
      return;
    }

    this.creating.set(true);
    this.error.set(null);
    try {
      const now = new Date().toISOString();
      const character: Record<string, unknown> = {
        npc: true,
        name,
        description,
        // v4 `:167` — the one field fills both vantage points.
        personality: description,
        defaultConnectionProfileId: profileId,
      };

      const scenario = this.scenario().trim();
      if (scenario) {
        // v4 `:172-176`: a one-element `scenarios[]`; a scalar `scenario` is
        // silently stripped by the server schema.
        character['scenarios'] = [
          { id: crypto.randomUUID(), title: 'Default', content: scenario },
        ];
      }

      const systemPrompt = this.systemPrompt().trim();
      if (systemPrompt) {
        character['systemPrompts'] = [
          {
            id: crypto.randomUUID(),
            name: 'Default',
            content: systemPrompt,
            isDefault: true,
            createdAt: now,
            updatedAt: now,
          },
        ];
      }

      const physical = this.physicalDescription().trim();
      if (physical) {
        // v4 `:191-203`: the SINGULAR object; the plural array is stripped.
        character['physicalDescription'] = {
          id: crypto.randomUUID(),
          name: 'Default',
          fullDescription: physical,
          createdAt: now,
          updatedAt: now,
        };
      }

      const data = await this.core.dispatchData({ type: 'characterCreate', character });
      const created = (data['character'] as { id?: string } | undefined) ?? data;
      const characterId = String((created as { id?: string }).id ?? '');
      if (!characterId) {
        throw new Error('Failed to create NPC');
      }

      // The avatar, best-effort — v4 logs a failure and carries on (`:236-241`).
      const file = this.avatarFile();
      if (file) {
        try {
          const photo = await uploadCharacterPhoto(characterId, file);
          await this.core.dispatchData({
            type: 'characterAvatar',
            characterId,
            imageId: photo.linkId,
          });
        } catch {
          // Deliberately swallowed: the NPC exists and is about to join.
        }
      }

      this.created.emit(characterId);
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to create NPC');
    } finally {
      this.creating.set(false);
    }
  }
}
