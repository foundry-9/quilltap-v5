import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../../core/core-client';
import type { ChatSetScenarioResult } from '../../core/core-contract';
import {
  fetchCharacterScenarios,
  fetchGeneralScenarios,
  fetchGroupScenarios,
  fetchProjectScenarios,
  scenarioKeys,
} from '../../scenario/scenario.api';
import { ScenarioSelect, hasAnyScenarioOptions } from '../../scenario/scenario-select';
import {
  scenarioSelectionToPayload,
  type CharacterScenario,
  type GeneralScenarioOption,
  type GroupScenarioOption,
  type ProjectScenarioOption,
  type ScenarioSelection,
} from '../../scenario/scenario.types';
import { ToastService } from '../../ui/toast.service';

/** What the user has picked in this sitting (v4's `draft` state). */
interface ScenarioDraft {
  selection: ScenarioSelection;
  customText: string;
}

/**
 * The in-chat scenario picker, living in the Salon sidebar's Chat section (v4
 * `components/chat/ChatScenarioControl.tsx`, NEW at `44a8137e`).
 *
 * Offers the same four tiers the New Chat dialog does — project, general,
 * group, character — through the shared {@link ScenarioSelect}, plus a
 * free-text box that appears only for "Custom…". Saving dispatches
 * `chatSetScenario` (v4 `?action=scenario`), which rewrites `chat.scenarioText`,
 * recompiles every participant's identity stack, and has the Host announce the
 * revision.
 *
 * The current scene seeds the control: when its text matches a preset exactly,
 * that preset is preselected; otherwise the control opens on "Custom…" with the
 * text in the box, ready to edit. **That seeding is the bug `44a8137e` fixes** —
 * v4's chat GET never projected `scenarioText`, so the picker could only ever
 * open on "Custom…", even immediately after a save.
 *
 * The seed is DERIVED, not copied into state, so it settles as the option tiers
 * finish loading without ever fighting a choice the user has since made; a save
 * drops the draft so the refetched chat is the authority on what the scene now
 * is.
 */
@Component({
  selector: 'qt-chat-scenario-control',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block' },
  imports: [ScenarioSelect],
  template: `
    <div class="qt-label">
      <span class="block mb-1">Scenario</span>
      @if (showDropdown()) {
        <qt-scenario-select
          selectId="chat-scenario-select"
          [selection]="selection()"
          (selectionChange)="setSelection($event)"
          [projectScenarios]="projectScenarios()"
          [generalScenarios]="generalScenarios()"
          [groupScenarios]="groupScenarios()"
          [characterScenarios]="offeredCharacterScenarios()"
          [disabled]="saving()"
          selectClass="qt-select text-sm mb-2"
          ariaLabel="Scenario"
        />
      }
      @if (selection().kind === 'custom') {
        <textarea
          class="qt-textarea text-sm"
          rows="6"
          aria-label="Custom scenario text"
          placeholder="Set the scene for this conversation…"
          [value]="customText()"
          [disabled]="saving()"
          (input)="setCustomText($event)"
        ></textarea>
      } @else if (selectedPresetBody(); as body) {
        <div
          class="max-h-40 overflow-y-auto rounded-lg border qt-border-default qt-bg-muted/40 px-2 py-1.5 text-xs qt-text-secondary whitespace-pre-wrap"
        >
          {{ body }}
        </div>
      }
      <button
        type="button"
        class="qt-tool-palette-button mt-2"
        [disabled]="saving()"
        (click)="save()"
      >
        {{ saving() ? 'Setting the scene…' : 'Change scenario' }}
      </button>
      <span class="block mt-1 qt-text-secondary text-xs">
        Changing the scene rewrites every character&rsquo;s standing instructions, and the Host
        announces the revision to the company.
      </span>
    </div>
  `,
})
export class ChatScenarioControl {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();
  /** The chat's project, when it has one — gates the project tier. */
  readonly projectId = input<string | null>(null);
  /** The scene as currently stored on the chat (v4 `44a8137e`'s GET projection). */
  readonly scenarioText = input<string | null>(null);
  /** Character IDs of the LLM-controlled cast, for the group tier. */
  readonly llmCharacterIds = input<readonly string[]>([]);
  /**
   * The single LLM character's ID, or null when the room holds several. Only a
   * one-character room can offer character scenarios unambiguously.
   */
  readonly singleLlmCharacterId = input<string | null>(null);
  /** Gates the reference-data fetches until the section has been opened once. */
  readonly enabled = input(false);
  /** Fired after the scenario is saved (v4 `onChatUpdated`, typically `fetchChat`). */
  readonly chatUpdated = output<void>();

  private readonly draft = signal<ScenarioDraft | null>(null);
  protected readonly saving = signal(false);

  /** v4's `characterIdsKey`: the comma-joined, SORTED ids the group tier keys on. */
  private readonly characterIdsKey = computed(() => [...this.llmCharacterIds()].sort().join(','));

  private readonly generalQuery = injectQuery(() => ({
    queryKey: scenarioKeys.general,
    queryFn: (): Promise<GeneralScenarioOption[]> => fetchGeneralScenarios(this.core),
    enabled: this.enabled(),
  }));

  private readonly projectQuery = injectQuery(() => ({
    queryKey: scenarioKeys.project(this.projectId() ?? ''),
    queryFn: (): Promise<ProjectScenarioOption[]> =>
      fetchProjectScenarios(this.core, this.projectId()!),
    enabled: this.enabled() && Boolean(this.projectId()),
  }));

  private readonly groupQuery = injectQuery(() => ({
    queryKey: scenarioKeys.group(this.characterIdsKey()),
    queryFn: (): Promise<GroupScenarioOption[]> =>
      fetchGroupScenarios(this.core, [...this.llmCharacterIds()].sort()),
    enabled: this.enabled() && this.characterIdsKey().length > 0,
  }));

  private readonly characterQuery = injectQuery(() => ({
    queryKey: scenarioKeys.character(this.singleLlmCharacterId() ?? ''),
    queryFn: (): Promise<CharacterScenario[]> =>
      fetchCharacterScenarios(this.core, this.singleLlmCharacterId()!),
    enabled: this.enabled() && Boolean(this.singleLlmCharacterId()),
  }));

  protected readonly generalScenarios = computed(() => this.generalQuery.data() ?? []);
  protected readonly projectScenarios = computed(() =>
    this.projectId() ? (this.projectQuery.data() ?? []) : [],
  );
  protected readonly groupScenarios = computed(() => this.groupQuery.data() ?? []);
  protected readonly characterScenarios = computed(() =>
    this.singleLlmCharacterId() ? (this.characterQuery.data() ?? []) : [],
  );

  /**
   * v4 passes `singleLlmCharacterId ? characterScenarios : null` — the tier is
   * withheld, not merely empty, when several characters share the room.
   *
   * ⚠ This gate is REDUNDANT in v4 and redundant here: {@link characterScenarios}
   * is already gated on the same id, and the query behind it does not even fire
   * without one. Transcribed anyway because it is v4's spelling; a mutation
   * that drops it survives (correctly), while dropping the query's own gate
   * reddens the withholding arm. The load-bearing gate is the `enabled` above.
   */
  protected readonly offeredCharacterScenarios = computed<CharacterScenario[] | null>(() =>
    this.singleLlmCharacterId() ? this.characterScenarios() : null,
  );

  protected readonly showDropdown = computed(() =>
    hasAnyScenarioOptions({
      projectScenarios: this.projectScenarios(),
      generalScenarios: this.generalScenarios(),
      groupScenarios: this.groupScenarios(),
      characterScenarios: this.characterScenarios(),
    }),
  );

  /**
   * The scene the chat already has, expressed as a picker state: an exact body
   * match preselects that preset, anything else (including a preset with notes
   * layered beneath it) reads as Custom with the text ready to edit. Derived
   * rather than seeded into state, so it settles as the tiers finish loading
   * without ever fighting a choice the user has since made.
   */
  private readonly currentAsSelection = computed<ScenarioDraft>(() => {
    const current = (this.scenarioText() ?? '').trim();
    if (current.length === 0) return { selection: { kind: 'custom' }, customText: '' };

    const projectMatch = this.projectScenarios().find((s) => s.body.trim() === current);
    if (projectMatch) {
      return { selection: { kind: 'project', path: projectMatch.path }, customText: '' };
    }

    const generalMatch = this.generalScenarios().find((s) => s.body.trim() === current);
    if (generalMatch) {
      return { selection: { kind: 'general', path: generalMatch.path }, customText: '' };
    }

    const groupMatch = this.groupScenarios().find((s) => s.body.trim() === current);
    if (groupMatch) {
      return {
        selection: { kind: 'group', groupId: groupMatch.groupId, path: groupMatch.path },
        customText: '',
      };
    }

    const characterMatch = this.characterScenarios().find((s) => s.content.trim() === current);
    if (characterMatch) {
      return { selection: { kind: 'character', scenarioId: characterMatch.id }, customText: '' };
    }

    return { selection: { kind: 'custom' }, customText: this.scenarioText() ?? '' };
  });

  private readonly current = computed<ScenarioDraft>(
    () => this.draft() ?? this.currentAsSelection(),
  );
  protected readonly selection = computed(() => this.current().selection);
  protected readonly customText = computed(() => this.current().customText);

  /**
   * The body behind the current selection, previewed under the dropdown the way
   * the New Chat dialog previews it.
   */
  protected readonly selectedPresetBody = computed<string | null>(() => {
    const selection = this.selection();
    switch (selection.kind) {
      case 'project':
        return this.projectScenarios().find((s) => s.path === selection.path)?.body ?? null;
      case 'general':
        return this.generalScenarios().find((s) => s.path === selection.path)?.body ?? null;
      case 'group':
        return (
          this.groupScenarios().find(
            (s) => s.path === selection.path && s.groupId === selection.groupId,
          )?.body ?? null
        );
      case 'character':
        return this.characterScenarios().find((s) => s.id === selection.scenarioId)?.content ?? null;
      default:
        return null;
    }
  });

  protected setSelection(next: ScenarioSelection): void {
    this.draft.set({ selection: next, customText: this.customText() });
  }

  protected setCustomText(event: Event): void {
    const next = (event.target as HTMLTextAreaElement).value;
    this.draft.set({ selection: this.selection(), customText: next });
  }

  /**
   * v4 `handleSave`. The success toast is whatever the server called it —
   * "Scenario updated" / "Scenario cleared" / "Scenario unchanged", the last
   * being the no-op arm — falling back to v4's own `'Scenario updated'`. The
   * draft is dropped so the control re-derives from what actually persisted;
   * the refetched chat is the authority on what the scene now is.
   */
  protected async save(): Promise<void> {
    const selection = this.selection();
    try {
      this.saving.set(true);
      const payload = scenarioSelectionToPayload(selection);
      const data = await this.core.dispatchData({
        type: 'chatSetScenario',
        chatId: this.chatId(),
        ...payload,
        // Free text is the WHOLE scene only under Custom; an empty box clears
        // the scene, which is why it is sent even when blank.
        ...(selection.kind === 'custom' ? { scenario: this.customText() } : {}),
      });
      const message = (data as unknown as ChatSetScenarioResult).message;
      this.toasts.showSuccess(message || 'Scenario updated');
      this.draft.set(null);
      this.chatUpdated.emit();
    } catch (error) {
      this.toasts.showError(coreErrorMessage(error, 'Failed to update scenario'));
    } finally {
      this.saving.set(false);
    }
  }
}
