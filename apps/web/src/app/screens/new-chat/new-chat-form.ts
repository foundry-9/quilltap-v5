import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { AutonomousRoomCard } from '../../autonomous/autonomous-room-card';
import type {
  AutonomousSettingsHint,
  NewChatAutonomousState,
} from '../../autonomous/autonomous.logic';
import { CoreClient } from '../../core/core-client';
import { MarkdownField } from '../../editor/markdown-field';
import type {
  ChatCreateOutfitSelectionInput,
  ChatSettingsDto,
  TimestampConfig,
} from '../../core/core-contract';
import { Icon } from '../../ui/icon';
import { ImageProfilePicker } from '../../images/image-profile-picker';
import { ScenarioSelect, hasAnyScenarioOptions } from '../../scenario/scenario-select';
import type { ScenarioSelection } from '../../scenario/scenario.types';
import { applyPlayAs, scenarioSelectionPatch } from './new-chat.logic';
import { NewChatState } from './new-chat.state';
import {
  type CharacterScenarioOption,
  type GroupScenarioOption,
  type NewChatSelectedCharacter,
  type ScenarioOption,
} from './new-chat.types';
import { OutfitSelector, type OutfitSelectorCharacter } from './outfit-selector';
import { TimestampConfigCard } from './timestamp-config-card';

interface PlayAsOption {
  id: string;
  label: string;
}

/**
 * The New-Chat form body (v4 `components/new-chat/NewChatForm.tsx`): the Play-As
 * select, the image-profile picker, the four-source scenario dropdown +
 * read-only preview + free-text notes (layered), the outfit selector, the
 * avatar toggle, the Reality-Injection timestamp card, and the project row.
 * Autonomous mode is a loud disabled-with-title deferral.
 *
 * ## The group optgroup, and "Show archived" (v4 `d25dacc1`)
 *
 * The group-scenario optgroup used to be absent here, faithfully: v4's
 * `NewChatForm` declared a `groupScenarios` prop and (since `44a8137e`) handed
 * it to the shared `<ScenarioSelect>`, but NEITHER caller ever passed it, so
 * the prop defaulted to `[]` and v4 rendered no group optgroup either.
 * `d25dacc1` connected `NewChatPageClient` to it, so the tier is now WIRED —
 * which also makes the group arms of the preset preview and the selection
 * chain reachable for the first time.
 *
 * **Recorded mechanism divergence — the "Show archived" checkbox.** v4 renders
 * it only when its change-callback prop is supplied, so the second caller
 * (`NewChatModal`) is a pure consumer of whatever tiers it was handed. v5 never
 * ported that modal (the standing no-modal divergence — this screen IS the
 * `salon-new` tab), and v5's form reads {@link NewChatState} directly rather
 * than taking scenario props, so there is no caller that could withhold the
 * setter. The checkbox therefore always renders — which is exactly what v4's
 * page, the one caller v5 has, does.
 */
@Component({
  selector: 'qt-new-chat-form',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    FormsModule,
    Icon,
    ImageProfilePicker,
    MarkdownField,
    OutfitSelector,
    ScenarioSelect,
    TimestampConfigCard,
    AutonomousRoomCard,
  ],
  template: `
    <div class="new-chat-form grid grid-cols-1 md:grid-cols-2 gap-6 items-stretch">
      <!-- Autonomous toggle (spans both columns) -->
      <div class="md:col-span-2 rounded-xl border qt-border-default qt-bg-card/60 p-4">
        <label class="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            [checked]="isAutonomous()"
            (change)="onAutonomousToggle($any($event.target).checked)"
            [disabled]="creating() || hasUserControlled()"
            class="qt-checkbox mt-1"
          />
          <span>
            <span class="font-medium text-foreground">Make this an autonomous room</span>
            <span class="block qt-text-xs qt-text-muted mt-1">
              Autonomous rooms run when scheduled or started manually. They have no human user, no
              composer, and pause for nobody.
            </span>
          </span>
        </label>
        @if (hasUserControlled() && !isAutonomous()) {
          <p class="mt-2 qt-text-xs qt-text-warning">
            A character is set to Play As (user). Autonomous rooms have no user — revert it to “Chat
            as yourself” to enable.
          </p>
        }
      </div>

      <!-- Left card: Character Customization -->
      <div class="rounded-xl border qt-border-default qt-bg-card p-6 space-y-4">
        <h3 class="qt-section-title">Character Customization</h3>

        @if (profiles().length === 0) {
          <div class="rounded-lg border qt-border-warning/50 qt-bg-warning/10 p-3 qt-text-warning">
            <p class="qt-label">No connection profiles available</p>
            <p class="mt-1 qt-body-sm">Add an AI provider in Settings to start a chat.</p>
          </div>
        }

        @if (playAsOptions().length > 0) {
          <div>
            <label for="new-chat-partner" class="mb-2 block text-sm qt-text-primary"
              >Play As (Optional)</label
            >
            <select
              id="new-chat-partner"
              [ngModel]="userEntry()?.character?.id ?? ''"
              (ngModelChange)="onPlayAs($event)"
              [disabled]="creating()"
              class="qt-select"
            >
              <option value="">Chat as yourself</option>
              @for (opt of playAsOptions(); track opt.id) {
                <option [value]="opt.id" [selected]="opt.id === (userEntry()?.character?.id ?? '')">
                  {{ opt.label }}
                </option>
              }
            </select>
          </div>
        }

        @if (roleplayTemplates().length > 0) {
          <div>
            <label for="new-chat-roleplay-template" class="mb-2 block text-sm qt-text-primary"
              >Roleplay Template</label
            >
            <select
              id="new-chat-roleplay-template"
              [ngModel]="form().roleplayTemplateId ?? ''"
              (ngModelChange)="onRoleplayTemplate($event)"
              [disabled]="creating()"
              class="qt-select"
            >
              <option value="">
                No Template{{ defaultRoleplayTemplateId() === null ? ' (default)' : '' }}
              </option>
              @for (template of roleplayTemplates(); track template.id) {
                <option [value]="template.id">
                  {{ template.name }}{{ template.isBuiltIn ? ' (Built-in)' : ''
                  }}{{ template.id === defaultRoleplayTemplateId() ? ' (default)' : '' }}
                </option>
              }
            </select>
            <p class="qt-text-xs qt-text-muted mt-1">
              Sets how prose, dialogue, and asides are dressed for this conversation. You may change
              your mind later from the chat&rsquo;s own sidebar.
            </p>
          </div>
        }

        <div>
          <label class="mb-2 block text-sm qt-text-primary"
            >Image Generation Profile (Optional)</label
          >
          <qt-image-profile-picker
            [value]="form().imageProfileId || null"
            [characterId]="characterIdForImage()"
            [disabled]="creating()"
            (changed)="onImageProfile($event)"
          />
        </div>

        <div>
          <label for="new-chat-scenario" class="mb-2 block text-sm qt-text-primary"
            >Starting Scenario (Optional)</label
          >
          @if (showScenarioDropdown()) {
            <qt-scenario-select
              selectId="new-chat-scenario-select"
              [selection]="scenarioSelection()"
              (selectionChange)="onScenarioSelect($event)"
              [projectScenarios]="projectScenarios()"
              [generalScenarios]="generalScenarios()"
              [groupScenarios]="groupScenarios()"
              [characterScenarios]="characterScenarios()"
              [characterDefaultScenarioId]="singleLlm()?.character?.defaultScenarioId ?? null"
              [disabled]="creating()"
            />
          }

          <label class="mb-2 flex items-center gap-2 text-xs qt-text-muted">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="showArchivedScenarios()"
              [disabled]="creating()"
              (change)="onShowArchivedScenarios($event)"
            />
            Show archived
          </label>

          @if (overrideNote(); as note) {
            <p class="mb-2 text-xs qt-text-muted">
              Using the project default. Character default:
              <button
                type="button"
                (click)="switchToCharacterDefault()"
                [disabled]="creating()"
                class="underline hover:no-underline qt-text-primary"
              >
                {{ note.title }}
              </button>
              — click to switch.
            </p>
          }

          @if (presetContent(); as preview) {
            <div
              class="rounded-lg border qt-border-default qt-bg-muted/40 px-3 py-2 text-sm qt-text-secondary whitespace-pre-wrap"
            >
              {{ preview }}
            </div>
            <p class="mb-1 mt-2 text-xs qt-text-muted">
              Your notes here are added beneath the scenario above.
            </p>
          }

          <qt-markdown-field
            minHeight="6rem"
            [ariaLabel]="presetContent() ? 'Additional scenario notes' : 'Starting scenario'"
            [value]="form().scenario"
            [disabled]="creating()"
            (contentChange)="onScenarioNotes($event)"
          />
        </div>

        @if (outfitCharacters().length > 0) {
          <qt-outfit-selector
            [characters]="outfitCharacters()"
            [disabled]="creating()"
            (selectionsChange)="onOutfits($event)"
          />
        }

        <div>
          <label class="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              [ngModel]="form().avatarGenerationEnabled"
              (ngModelChange)="onAvatarToggle($event)"
              [disabled]="creating()"
              class="qt-checkbox"
            />
            <span class="qt-text-small">Auto-generate character avatars</span>
          </label>
          <p class="qt-text-xs qt-text-muted mt-1">
            Generate new portraits when outfits change (uses image API)
          </p>
        </div>
      </div>

      <!-- Right card: Reality Injection Mode (chat) or Autonomous Room (autonomous) -->
      @if (isAutonomous()) {
        <qt-autonomous-room-card
          [value]="form().autonomous"
          [settingsHint]="settingsHint()"
          [disabled]="creating()"
          (changed)="updateAutonomous($event)"
        />
      } @else {
        <div class="rounded-xl border qt-border-default qt-bg-card p-6 space-y-4">
          <h3 class="qt-section-title">Reality Injection Mode</h3>
          <qt-timestamp-config-card
            [value]="form().timestampConfig"
            [disabled]="creating()"
            (changed)="onTimestamp($event)"
          />
        </div>
      }

      <!-- Project row -->
      @if (availableProjects().length > 0) {
        <div class="md:col-span-2 rounded-lg border qt-border-default qt-bg-card/50 p-3 space-y-2">
          <label for="new-chat-project-select" class="qt-text-xs qt-text-muted"
            >File this chat under a project</label
          >
          <div class="flex items-center gap-3">
            <div
              class="w-6 h-6 rounded flex items-center justify-center flex-shrink-0"
              [style.background-color]="projectColor() || 'var(--muted)'"
            >
              <qt-icon name="folder" class="w-3 h-3 qt-text-secondary" />
            </div>
            <select
              id="new-chat-project-select"
              [ngModel]="selectedProjectId() ?? ''"
              (ngModelChange)="onProject($event)"
              [disabled]="creating()"
              class="qt-select flex-1 min-w-0"
            >
              <option value="">— None (General) —</option>
              @for (p of availableProjects(); track p.id) {
                <option [value]="p.id" [selected]="p.id === (selectedProjectId() ?? '')">
                  {{ p.name }}
                </option>
              }
            </select>
          </div>
        </div>
      } @else if (project(); as proj) {
        <div class="md:col-span-2 rounded-lg border qt-border-default qt-bg-card/50 p-3">
          <div class="flex items-center gap-3">
            <div
              class="w-6 h-6 rounded flex items-center justify-center flex-shrink-0"
              [style.background-color]="proj.color || 'var(--muted)'"
            >
              <qt-icon name="folder" class="w-3 h-3 qt-text-secondary" />
            </div>
            <div class="min-w-0">
              <p class="qt-text-xs qt-text-muted">In project</p>
              <p class="qt-body truncate">{{ proj.name }}</p>
            </div>
          </div>
        </div>
      }
    </div>
  `,
})
export class NewChatForm {
  readonly state = input.required<NewChatState>();

  private readonly coreClient = inject(CoreClient);

  private core(): NewChatState {
    return this.state();
  }

  protected form = computed(() => this.core().form());
  protected profiles = computed(() => this.core().profiles());
  protected creating = computed(() => this.core().creating());
  protected projectScenarios = computed(() => this.core().projectScenarios());
  protected generalScenarios = computed(() => this.core().generalScenarios());
  protected groupScenarios = computed(() => this.core().groupScenarios());
  protected showArchivedScenarios = computed(() => this.core().showArchivedScenarios());
  protected availableProjects = computed(() => this.core().availableProjects());
  protected selectedProjectId = computed(() => this.core().selectedProjectId());
  protected project = computed(() => this.core().project());
  /** The picker's options; an empty list hides the dropdown entirely (v4). */
  protected roleplayTemplates = computed(() => this.core().roleplayTemplates());
  /** Marks the option the chat would get anyway — labelling only. */
  protected defaultRoleplayTemplateId = computed(() => this.core().defaultRoleplayTemplateId());

  protected readonly llmSelected = computed(() => this.core().llmSelected());
  protected readonly singleLlm = computed<NewChatSelectedCharacter | null>(() =>
    this.llmSelected().length === 1 ? this.llmSelected()[0] : null,
  );
  protected readonly userEntry = computed<NewChatSelectedCharacter | undefined>(() =>
    this.core()
      .selectedCharacters()
      .find((sc) => sc.controlledBy === 'user'),
  );

  // --- Autonomous room --------------------------------------------------------

  protected readonly isAutonomous = computed(() => this.form().autonomous.enabled);
  protected readonly hasUserControlled = computed(() =>
    this.core()
      .selectedCharacters()
      .some((sc) => sc.controlledBy === 'user'),
  );

  /** The user-level autonomous defaults that soften the card's placeholders (v4 `autonomousSettingsHint`). */
  private readonly settingsQuery = injectQuery(() => ({
    queryKey: ['chatSettings'],
    queryFn: async (): Promise<ChatSettingsDto> => {
      const resp = await this.coreClient.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      return resp.data;
    },
  }));

  protected readonly settingsHint = computed<AutonomousSettingsHint | undefined>(() => {
    const ar = (this.settingsQuery.data()?.['autonomousRoomSettings'] ?? {}) as {
      visibilityDefault?: AutonomousSettingsHint['visibilityDefault'];
      destructiveToolPolicy?: AutonomousSettingsHint['destructiveToolPolicy'];
      defaultFreshnessWindowMs?: number;
    };
    return {
      visibilityDefault: ar.visibilityDefault,
      destructiveToolPolicy: ar.destructiveToolPolicy,
      defaultFreshnessHours:
        typeof ar.defaultFreshnessWindowMs === 'number' && ar.defaultFreshnessWindowMs > 0
          ? Math.round(ar.defaultFreshnessWindowMs / (60 * 60 * 1000))
          : undefined,
    };
  });

  /**
   * Every scenario on the single LLM character, archived ones included (v4
   * `allCharacterScenarios`). The character record always carries them — the
   * vault projection sweeps files missing from the array — so the hiding
   * happens here, not at the vault read.
   */
  protected readonly allCharacterScenarios = computed<CharacterScenarioOption[]>(
    () => this.singleLlm()?.character.scenarios ?? [],
  );

  /**
   * What the dropdown offers (v4 `singleCharacterScenarios`). Archived
   * scenarios are hidden unless "Show archived" is ticked — with ONE exception:
   * whatever is currently selected always stays in the list, so an archived
   * pick made a moment ago does not blank the select out from under the user.
   * (That exception is also what keeps the UNGUARDED character default seed
   * visible; see `NewChatState.seedFromCharacter`.)
   */
  protected readonly characterScenarios = computed<CharacterScenarioOption[]>(() => {
    if (!this.singleLlm()) return [];
    const showArchived = this.showArchivedScenarios();
    const selectedId = this.form().scenarioId;
    return this.allCharacterScenarios().filter(
      (s) => showArchived || s.archived !== true || s.id === selectedId,
    );
  });

  protected readonly showScenarioDropdown = computed(() =>
    hasAnyScenarioOptions({
      projectScenarios: this.projectScenarios(),
      generalScenarios: this.generalScenarios(),
      groupScenarios: this.groupScenarios(),
      characterScenarios: this.characterScenarios(),
    }),
  );

  private readonly selectedProjectScenario = computed<ScenarioOption | undefined>(() =>
    this.form().projectScenarioPath
      ? this.projectScenarios().find((s) => s.path === this.form().projectScenarioPath)
      : undefined,
  );
  private readonly selectedGeneralScenario = computed<ScenarioOption | undefined>(() =>
    this.form().generalScenarioPath
      ? this.generalScenarios().find((s) => s.path === this.form().generalScenarioPath)
      : undefined,
  );
  private readonly selectedGroupScenario = computed<GroupScenarioOption | undefined>(() =>
    this.form().groupScenarioPath
      ? this.groupScenarios().find(
          (s) =>
            s.path === this.form().groupScenarioPath &&
            s.groupId === this.form().groupScenarioGroupId,
        )
      : undefined,
  );
  /**
   * Looked up against the UNFILTERED list (v4 `:191-194`): a scenario the form
   * already points at keeps previewing even after it has been archived.
   */
  private readonly selectedCharacterScenario = computed<CharacterScenarioOption | undefined>(() =>
    this.form().scenarioId
      ? this.allCharacterScenarios().find((s) => s.id === this.form().scenarioId)
      : undefined,
  );

  protected readonly presetContent = computed<string | null>(() => {
    if (this.selectedProjectScenario()) return this.selectedProjectScenario()!.body;
    if (this.selectedGeneralScenario()) return this.selectedGeneralScenario()!.body;
    if (this.selectedGroupScenario()) return this.selectedGroupScenario()!.body;
    if (this.selectedCharacterScenario()) return this.selectedCharacterScenario()!.content;
    return null;
  });

  /**
   * The selection the dropdown shows, in v4's exact precedence order (v4
   * `44a8137e`'s `scenarioSelection` — the same chain its `dropdownValue`
   * expressed as tokens before the extraction).
   */
  protected readonly scenarioSelection = computed<ScenarioSelection>(() => {
    const project = this.selectedProjectScenario();
    if (project) return { kind: 'project', path: project.path };
    const general = this.selectedGeneralScenario();
    if (general) return { kind: 'general', path: general.path };
    const group = this.selectedGroupScenario();
    if (group) return { kind: 'group', groupId: group.groupId, path: group.path };
    const character = this.selectedCharacterScenario();
    if (character) return { kind: 'character', scenarioId: character.id };
    return { kind: 'custom' };
  });

  private readonly characterDefaultScenario = computed<CharacterScenarioOption | undefined>(() => {
    const id = this.singleLlm()?.character.defaultScenarioId;
    if (!id) return undefined;
    return this.characterScenarios().find((s) => s.id === id);
  });

  protected readonly overrideNote = computed<{ title: string } | null>(() =>
    this.selectedProjectScenario() && this.characterDefaultScenario()
      ? { title: this.characterDefaultScenario()!.title }
      : null,
  );

  protected readonly outfitCharacters = computed<OutfitSelectorCharacter[]>(() => {
    const list: OutfitSelectorCharacter[] = this.llmSelected().map((sc) => ({
      id: sc.character.id,
      name: sc.character.name,
      isUserControlled: false,
      canChooseOutfit: sc.character.canChooseOutfit ?? false,
    }));
    const ue = this.userEntry();
    if (ue) {
      list.push({
        id: ue.character.id,
        name: ue.character.name,
        isUserControlled: true,
        canChooseOutfit: ue.character.canChooseOutfit ?? false,
      });
    }
    return list;
  });

  protected readonly characterIdForImage = computed<string | undefined>(
    () =>
      this.singleLlm()?.character.id ??
      this.core().selectedCharacters()[0]?.character.id ??
      undefined,
  );

  protected readonly projectColor = computed<string | null | undefined>(
    () => this.availableProjects().find((p) => p.id === this.selectedProjectId())?.color,
  );

  /**
   * Play-As options: only characters already in the cast (v4 `e2eb3d21`). Any
   * added character can take the user's chair, and default-user personas now
   * appear in the picker on the left, so they enter the cast the same way as
   * everyone else rather than being pulled in from a separate roster here.
   */
  protected readonly playAsOptions = computed<PlayAsOption[]>(() => {
    const dupNames = this.duplicateUserNames();
    return this.core()
      .selectedCharacters()
      .map((sc) => ({
        id: sc.character.id,
        label: this.formatName(sc.character.name, sc.character.title, dupNames),
      }));
  });

  private duplicateUserNames(): Set<string> {
    const counts = new Map<string, number>();
    for (const c of this.core().userControlledCharacters()) {
      counts.set(c.name, (counts.get(c.name) ?? 0) + 1);
    }
    const dup = new Set<string>();
    for (const [name, n] of counts) if (n > 1) dup.add(name);
    return dup;
  }

  private formatName(name: string, title: string | null | undefined, dup: Set<string>): string {
    return title && dup.has(name) ? `${name} (${title})` : name;
  }

  // --- Handlers --------------------------------------------------------------

  protected onPlayAs(nextId: string): void {
    this.core().setSelectedCharacters((prev) => applyPlayAs(prev, nextId));
  }

  protected onScenarioSelect(selection: ScenarioSelection): void {
    this.core().patchForm(scenarioSelectionPatch(selection));
  }

  protected switchToCharacterDefault(): void {
    const def = this.characterDefaultScenario();
    if (!def) return;
    this.core().patchForm({
      scenarioId: def.id,
      projectScenarioPath: null,
      generalScenarioPath: null,
      groupScenarioPath: null,
      groupScenarioGroupId: null,
      scenario: '',
    });
  }

  protected onShowArchivedScenarios(event: Event): void {
    void this.core().setShowArchivedScenarios((event.target as HTMLInputElement).checked);
  }

  protected onScenarioNotes(value: string): void {
    this.core().patchForm({ scenario: value });
  }

  protected onImageProfile(id: string | null): void {
    this.core().patchForm({ imageProfileId: id || '' });
  }

  /**
   * v4 `handleRoleplayTemplateChange` — the empty option means "no template"
   * (`null`), and ANY hand pick latches `roleplayTemplateTouched` so a later
   * reference-data reload cannot quietly re-seed the default over it.
   */
  protected onRoleplayTemplate(value: string): void {
    this.core().patchForm({
      roleplayTemplateId: value || null,
      roleplayTemplateTouched: true,
    });
  }

  protected onOutfits(selections: ChatCreateOutfitSelectionInput[]): void {
    this.core().patchForm({ outfitSelections: selections });
  }

  protected onAvatarToggle(enabled: boolean): void {
    this.core().patchForm({ avatarGenerationEnabled: enabled });
  }

  protected onTimestamp(config: TimestampConfig): void {
    this.core().patchForm({ timestampConfig: config });
  }

  protected onProject(id: string): void {
    void this.core().setSelectedProjectId(id || null);
  }

  protected onAutonomousToggle(next: boolean): void {
    this.core().patchForm({ autonomous: { ...this.form().autonomous, enabled: next } });
  }

  protected updateAutonomous(patch: Partial<NewChatAutonomousState>): void {
    this.core().patchForm({ autonomous: { ...this.form().autonomous, ...patch } });
  }
}
