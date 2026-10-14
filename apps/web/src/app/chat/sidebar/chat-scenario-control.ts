import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../../core/core-client';
import type {
  ChatSetScenarioRequest,
  ChatSetScenarioResult,
  ScenarioSourceInfo,
} from '../../core/core-contract';
import {
  fetchCharacterScenarios,
  fetchGeneralScenarios,
  fetchGroupScenarios,
  fetchProjectScenarios,
  fetchScenarioSource,
  injectScenarioRefBody,
  scenarioKeys,
} from '../../scenario/scenario.api';
import { ScenarioBrowserDialog } from '../../scenario/scenario-browser-dialog';
import type { ScenarioHierarchy } from '../../scenario/scenario-browser.logic';
import { ScenarioSelect } from '../../scenario/scenario-select';
import { chatKeys } from '../chat-keys';
import { HOST_AVATAR } from '../../scenario-builder/host-avatar';
import { injectImagesHidden } from '../hidden-image/images-hidden';
import {
  ScenarioBuilderDialog,
  type SavedScenarioTarget,
  type ScenarioBuilderCastMember,
} from '../../scenario-builder/scenario-builder-dialog';
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
 * group, character — through the shared {@link ScenarioSelect}, plus "Browse
 * all scenarios…", which opens the scenario browser so a scene from any shelf
 * in the instance can be plucked in (v4 `545e299bb`). Under a preset sits a
 * "Notes beneath the scene" box; under "Custom…" the box is the whole scene.
 * Saving dispatches `chatSetScenario` (v4 `?action=scenario`), which rewrites
 * `chat.scenarioText` (with its source and notes), recompiles every
 * participant's identity stack, and has the Host announce the revision.
 *
 * The current scene seeds the control. When the chat recorded where its scene
 * came from (`chatGetScenarioSource`, v4 `?action=scenario-source`), that
 * source is the authority: the provenance line names it, says when the shelf
 * copy has since been revised or removed, and offers "Reload from source"
 * (`reload: true`; an ordinary save keeps the scene the chat has) — the same
 * file, with the same notes beneath. A chat with no source (a custom scene, one
 * from before sources were recorded, or a server that does not answer the verb
 * yet) falls back to matching the text against the presets: an exact match
 * preselects that preset; otherwise the control opens on "Custom…" with the
 * text in the box, ready to edit. **That body match is the bug `44a8137e`
 * fixed** — v4's chat GET never projected `scenarioText`, so the picker could
 * only ever open on "Custom…", even immediately after a save.
 *
 * The seed is DERIVED, not copied into state, so it settles as the option tiers
 * finish loading without ever fighting a choice the user has since made; a save
 * drops the draft so the refetched chat is the authority on what the scene now
 * is.
 *
 * "Show archived" (v4 `d25dacc1`) re-fetches all four tiers with
 * `includeArchived`; the flag is part of each query key, so the two answers
 * cache separately instead of one overwriting the other.
 *
 * **"Ask the Host to set the scene"** (v4 `d1c06cd9d`, P4.D218): the Scenario
 * Builder, `@defer`red (v4's `next/dynamic`). "Use this scene" makes the
 * Host's draft the CUSTOM text — nothing is saved; the existing Change-scenario
 * button persists it, so the Host's revision announcement fires exactly as for
 * a hand-typed scene. After a "Save as scenario…" the new preset is selected
 * when this picker offers it, read from the caches the save dialog has
 * already invalidated and awaited.
 */
@Component({
  selector: 'qt-chat-scenario-control',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block' },
  imports: [ScenarioBrowserDialog, ScenarioBuilderDialog, ScenarioSelect],
  template: `
    <div class="qt-label">
      <span class="block mb-1">Scenario</span>
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
        offerBrowse
        (browse)="browserOpen.set(true)"
      />
      @if (browserOpen()) {
        <qt-scenario-browser-dialog
          [hierarchy]="hierarchy()"
          (closed)="browserOpen.set(false)"
          (pick)="setSelection($event)"
        />
      }
      @if (provenance(); as p) {
        <!-- v4 :411-428. The revised arm's trailing space is interpolated: a
             whitespace-only text node before the button would be dropped. -->
        <div class="mb-2 qt-text-secondary text-xs font-normal">
          From: <em>{{ p.ownerName }}</em> › <em>{{ p.name }}</em>{{ p.exists ? '' : ' — the shelf copy is gone' }}
          @if (p.exists && p.changed) {
            {{ ' — the shelf copy has been revised ' }}<button
              type="button"
              class="qt-button-secondary qt-button-sm ml-1"
              [disabled]="saving()"
              (click)="reload()"
            >
              Reload from source
            </button>
          }
        </div>
      }
      <label class="flex items-center gap-2 mb-2 qt-text-secondary text-xs font-normal">
        <input
          type="checkbox"
          class="qt-checkbox"
          [checked]="showArchived()"
          [disabled]="saving()"
          (change)="setShowArchived($event)"
        />
        Show archived
      </label>
      <button
        type="button"
        class="qt-button-secondary qt-button-sm inline-flex items-center gap-1.5 mb-2"
        [disabled]="saving()"
        (click)="builderOpen.set(true)"
      >
        <!-- Quick-hide "Salon Images" (v4 e3937d7aa ChatScenarioControl.tsx:
             349-355): the icon simply goes; the button text stays. -->
        @if (!imagesHidden()) {
          <img [src]="hostAvatar" alt="" class="h-4 w-4 rounded-full" />
        }
        Ask the Host to set the scene
      </button>
      @if (builderOpen()) {
        @defer {
          <qt-scenario-builder-dialog
            [cast]="castCharacters()"
            [projectId]="projectId() ?? null"
            [projectName]="projectName() ?? null"
            [chatId]="chatId()"
            (closed)="builderOpen.set(false)"
            (use)="handleUseBuiltScene($event)"
            (saved)="handleBuiltSceneSaved($event)"
          />
        }
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
      } @else {
        @if (previewBody(); as body) {
          <div
            class="max-h-40 overflow-y-auto rounded-lg border qt-border-default qt-bg-muted/40 px-2 py-1.5 text-xs qt-text-secondary whitespace-pre-wrap"
          >
            {{ body }}
          </div>
        }
        <span class="block mt-2 mb-1 text-xs">Notes beneath the scene</span>
        <textarea
          class="qt-textarea text-sm"
          rows="3"
          aria-label="Notes beneath the scene"
          placeholder="Anything to add beneath the preset (optional)…"
          [value]="customText()"
          [disabled]="saving()"
          (input)="setCustomText($event)"
        ></textarea>
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
  private readonly queryClient = injectQueryClient();

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
  /**
   * Every present character in the room — LLM- and user-controlled alike — for
   * the Scenario Builder: their vaults and groups scope what the Host may read,
   * and they are offered as save targets (v4 `castCharacters`, `d1c06cd9d`).
   */
  readonly castCharacters = input<readonly ScenarioBuilderCastMember[]>([]);
  /** The chat's project name, for the Scenario Builder's save targets. */
  readonly projectName = input<string | null>(null);
  /** Fired after the scenario is saved (v4 `onChatUpdated`, typically `fetchChat`). */
  readonly chatUpdated = output<void>();

  protected readonly hostAvatar = HOST_AVATAR;
  protected readonly imagesHidden = injectImagesHidden();
  protected readonly builderOpen = signal(false);
  /** v4 `browserOpen` — mounting the browser IS opening it (`@if` above). */
  protected readonly browserOpen = signal(false);

  private readonly draft = signal<ScenarioDraft | null>(null);
  protected readonly saving = signal(false);
  /**
   * v4 `:80`. Flipping it re-fetches every tier rather than filtering what is
   * already loaded — the server owns the hiding, so this list can never
   * disagree with the API.
   */
  protected readonly showArchived = signal(false);

  /** v4's `characterIdsKey`: the comma-joined, SORTED ids the group tier keys on. */
  private readonly characterIdsKey = computed(() => [...this.llmCharacterIds()].sort().join(','));

  private readonly generalQuery = injectQuery(() => ({
    queryKey: scenarioKeys.general(this.showArchived()),
    queryFn: (): Promise<GeneralScenarioOption[]> =>
      fetchGeneralScenarios(this.core, this.showArchived()),
    enabled: this.enabled(),
  }));

  private readonly projectQuery = injectQuery(() => ({
    queryKey: scenarioKeys.project(this.projectId() ?? '', this.showArchived()),
    queryFn: (): Promise<ProjectScenarioOption[]> =>
      fetchProjectScenarios(this.core, this.projectId()!, this.showArchived()),
    enabled: this.enabled() && Boolean(this.projectId()),
  }));

  private readonly groupQuery = injectQuery(() => ({
    queryKey: scenarioKeys.group(this.characterIdsKey(), this.showArchived()),
    queryFn: (): Promise<GroupScenarioOption[]> =>
      fetchGroupScenarios(this.core, [...this.llmCharacterIds()].sort(), this.showArchived()),
    enabled: this.enabled() && this.characterIdsKey().length > 0,
  }));

  private readonly characterQuery = injectQuery(() => ({
    queryKey: scenarioKeys.character(this.singleLlmCharacterId() ?? '', this.showArchived()),
    queryFn: (): Promise<CharacterScenario[]> =>
      fetchCharacterScenarios(this.core, this.singleLlmCharacterId()!, this.showArchived()),
    enabled: this.enabled() && Boolean(this.singleLlmCharacterId()),
  }));

  /**
   * Where the chat's scene came from (v4 `:181-186`, enabled with the section).
   * No query options of its own — v4 sets none, so the app defaults stand
   * (P4.D302 L-G, recorded). A server that does not answer the verb yet errors
   * here, and the control behaves as "no source": the body match below, no
   * provenance line — v4's `sourceInfo` undefined.
   */
  private readonly sourceQuery = injectQuery(() => ({
    queryKey: chatKeys.scenarioSource(this.chatId()),
    queryFn: (): Promise<ScenarioSourceInfo> => fetchScenarioSource(this.core, this.chatId()),
    enabled: this.enabled(),
  }));
  private readonly sourceInfo = computed(() => this.sourceQuery.data());

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

  /**
   * The group ids the browser counts as "in this chat" (v4 reads them off the
   * union envelope, `:213-216`). v5 caches the FLATTENED options, so they are
   * the distinct `groupId`s of that list — the union route skips empty groups,
   * so the two sets are identical (P4.D302 L-M, recorded).
   */
  private readonly groupIds = computed<string[]>(() => [
    ...new Set(this.groupScenarios().map((s) => s.groupId)),
  ]);

  /** The owners this picker already offers, for the browser (v4 `:405`). */
  protected readonly hierarchy = computed<ScenarioHierarchy>(() => ({
    projectId: this.projectId(),
    groupIds: this.groupIds(),
    characterId: this.singleLlmCharacterId(),
  }));

  /**
   * The scene the chat already has, expressed as a picker state. A recorded
   * source is the authority: it names the preset directly, and the notes are
   * what was layered beneath it. Without one, an exact body match preselects
   * that preset and anything else (including a preset with notes layered
   * beneath it) reads as Custom with the text ready to edit. Derived rather
   * than seeded into state, so it settles as the tiers finish loading without
   * ever fighting a choice the user has since made.
   */
  private readonly currentAsSelection = computed<ScenarioDraft>(() => {
    // Only a source whose file still exists can seed a preset: a save re-reads
    // the file, and a gone one would resolve to nothing, leaving the chat with
    // the notes alone. A gone source falls through to the text below, so the
    // scene the chat actually has stays in the box.
    const info = this.sourceInfo();
    if (info?.source && info.owner && info.exists) {
      const { source, owner } = info;
      const notes = info.notes ?? '';
      if (owner.scope === 'general') {
        return { selection: { kind: 'general', path: source.path }, customText: notes };
      }
      if (owner.scope === 'project' && owner.id && owner.id === this.projectId()) {
        return { selection: { kind: 'project', path: source.path }, customText: notes };
      }
      if (owner.scope === 'group' && owner.id && this.groupIds().includes(owner.id)) {
        return {
          selection: { kind: 'group', groupId: owner.id, path: source.path },
          customText: notes,
        };
      }
      if (
        owner.scope === 'character' &&
        owner.id &&
        owner.id === this.singleLlmCharacterId() &&
        info.scenarioId
      ) {
        return { selection: { kind: 'character', scenarioId: info.scenarioId }, customText: notes };
      }
      return {
        selection: {
          kind: 'store',
          mountPointId: source.mountPointId,
          path: source.path,
          label: `${owner.name} › ${info.scenarioName ?? source.path.replace(/^Scenarios\//, '').replace(/\.md$/i, '')}`,
        },
        customText: notes,
      };
    }

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
  private readonly selectedPresetBody = computed<string | null>(() => {
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
      case 'store':
      case 'custom':
      default:
        return null;
    }
  });

  /** A pick from elsewhere carries no body in the option lists; read it by ref (v4 `:312`). */
  private readonly storePreview = injectScenarioRefBody(
    () => {
      const selection = this.selection();
      return selection.kind === 'store'
        ? { mountPointId: selection.mountPointId, path: selection.path }
        : null;
    },
    () => this.enabled(),
  );

  protected readonly previewBody = computed<string | null>(() =>
    this.selection().kind === 'store'
      ? (this.storePreview.data()?.scenario.body ?? null)
      : this.selectedPresetBody(),
  );

  /**
   * The provenance line's facts (v4 `:411-428`): shown only when the chat
   * recorded a source AND an owner answered for it.
   */
  protected readonly provenance = computed(() => {
    const info = this.sourceInfo();
    if (!info?.source || !info.owner) return null;
    return {
      ownerName: info.owner.name,
      name: info.scenarioName ?? info.source.path,
      exists: info.exists,
      changed: info.changed,
    };
  });

  /**
   * "Use this scene" (v4 `handleUseBuiltScene`): the Host's draft becomes the
   * custom text. Nothing is dispatched — the existing Change-scenario button
   * persists it, so the Host's revision announcement fires exactly as for a
   * hand-typed scene.
   */
  protected handleUseBuiltScene(scene: string): void {
    this.draft.set({ selection: { kind: 'custom' }, customText: scene });
  }

  /**
   * After a save (v4 `handleBuiltSceneSaved`), select the new preset when this
   * picker offers it. The save dialog has already AWAITED the invalidation of
   * the scenario family, so the caches hold fresh lists — read them, never
   * refetch. v5's tier queries cache the flattened OPTION arrays (v4 caches
   * the raw `{ scenarios }` / `{ groupScenarios }` envelopes), so the lookups
   * read arrays; the rules are v4's: the project only for this chat's own
   * project, a group only when that group offers that path, a character only
   * when it is the room's lone LLM character.
   */
  protected handleBuiltSceneSaved(target: SavedScenarioTarget): void {
    const showArchived = this.showArchived();
    let next: ScenarioSelection | null = null;
    if (target.kind === 'general') {
      const data = this.queryClient.getQueryData<GeneralScenarioOption[]>(
        scenarioKeys.general(showArchived),
      );
      if ((data ?? []).some((s) => s.path === target.path)) {
        next = { kind: 'general', path: target.path };
      }
    } else if (target.kind === 'project' && target.projectId === this.projectId()) {
      const data = this.queryClient.getQueryData<ProjectScenarioOption[]>(
        scenarioKeys.project(target.projectId, showArchived),
      );
      if ((data ?? []).some((s) => s.path === target.path)) {
        next = { kind: 'project', path: target.path };
      }
    } else if (target.kind === 'group') {
      const data = this.queryClient.getQueryData<GroupScenarioOption[]>(
        scenarioKeys.group(this.characterIdsKey(), showArchived),
      );
      const offered = (data ?? []).some(
        (s) => s.groupId === target.groupId && s.path === target.path,
      );
      if (offered) next = { kind: 'group', groupId: target.groupId, path: target.path };
    } else if (target.kind === 'character' && target.characterId === this.singleLlmCharacterId()) {
      next = { kind: 'character', scenarioId: target.scenarioId };
    }
    if (next) this.draft.set({ selection: next, customText: '' });
  }

  protected setShowArchived(event: Event): void {
    this.showArchived.set((event.target as HTMLInputElement).checked);
  }

  /**
   * Under Custom the box is the whole scene; under a preset it is the notes
   * beneath it. Leaving Custom for a preset must not turn the old scene into
   * notes, or the new preset would be posted with the entire old scene layered
   * beneath it. Preset → preset (and preset → Custom) keep what was typed
   * (v4 `:281-288`). The CURRENT selection and text are read into locals
   * before the draft moves (P4.D302 L-H(1) — v4's closure saw the render's
   * values; a signal read after `.set` would see the new ones).
   */
  protected setSelection(next: ScenarioSelection): void {
    const selection = this.selection();
    const customText = this.customText();
    const text = selection.kind === 'custom' && next.kind !== 'custom' ? '' : customText;
    this.draft.set({ selection: next, customText: text });
  }

  protected setCustomText(event: Event): void {
    const next = (event.target as HTMLTextAreaElement).value;
    this.draft.set({ selection: this.selection(), customText: next });
  }

  /**
   * v4 `postScenario` (`:352-377`). The success toast is whatever the server
   * called it — "Scenario updated" / "Scenario cleared" / "Scenario
   * unchanged", the last being the no-op arm — falling back to v4's own
   * `'Scenario updated'`. A refusal toasts the server's own sentence verbatim
   * (P4.D302 L-K — e.g. `Invalid scenarioRef: …`; the SPA holds neither
   * refusal spelling). The order is v4's (L-H(3)): drop the draft so the
   * control re-derives from what actually persisted, AWAIT the source's
   * invalidation, THEN tell the parent to refetch the chat.
   */
  private async postScenario(
    payload: Omit<ChatSetScenarioRequest, 'type' | 'chatId'>,
  ): Promise<void> {
    const chatId = this.chatId();
    try {
      this.saving.set(true);
      const data = await this.core.dispatchData({ type: 'chatSetScenario', chatId, ...payload });
      const message = (data as unknown as ChatSetScenarioResult).message;
      this.toasts.showSuccess(message || 'Scenario updated');
      this.draft.set(null);
      await this.queryClient.invalidateQueries({ queryKey: chatKeys.scenarioSource(chatId) });
      this.chatUpdated.emit();
    } catch (error) {
      this.toasts.showError(coreErrorMessage(error, 'Failed to update scenario'));
    } finally {
      this.saving.set(false);
    }
  }

  /**
   * v4 `handleSave` (`:381-382`). Under a preset the text box holds the notes
   * beneath it; under Custom it is the whole scene (an empty box clears it).
   * Either way it travels as `scenario` — on EVERY save — and never `reload`.
   */
  protected save(): Promise<void> {
    return this.postScenario({
      ...scenarioSelectionToPayload(this.selection()),
      scenario: this.customText(),
    });
  }

  /**
   * v4 `handleReload` (`:385-389`) — the same file, re-read, with the same
   * notes beneath, so a reload is lossless. Exactly v4's three keys (L-L): no
   * pointer field, no `bodySha256`.
   */
  protected reload(): Promise<void> | undefined {
    const info = this.sourceInfo();
    if (!info?.source) return undefined;
    const { mountPointId, path } = info.source;
    return this.postScenario({
      scenarioRef: { mountPointId, path },
      scenario: info.notes ?? '',
      reload: true,
    });
  }
}
