/**
 * The New-Chat state service (v4 `components/new-chat/hooks/useNewChat.ts`): the
 * reference-data load, the seeding precedence, the form-state signals, and the
 * submit spine — as an Angular signals object the page owns. Not `@Injectable`:
 * it holds per-page state, so the page constructs one with the {@link CoreClient}
 * and the URL params.
 *
 * Faithful to v4's fetch/seed shape:
 *  - one batched load (characters / connection profiles / general scenarios /
 *    the project + its scenarios when a project is chosen / the seed character +
 *    its default partner / the participant-union group scenarios);
 *  - the seeding chain (pin 4): project default > general default; the seed
 *    character's default partner auto-joins as the user persona; the project
 *    LIST omits the default-* fields, so `projectGet` loads them;
 *  - the single-LLM default propagation (v4's post-load effect);
 *  - the submit spine (`handleCreate`) opening the Green Room BEFORE the
 *    dispatch and treating the dispatch resolving as the authoritative ready
 *    signal.
 *
 * The group-scenario union is fetched like v4 does but never rendered — v4's
 * `/salon/new` page fetches it in the hook yet never passes it into the form, so
 * the group optgroup is dead UI. Ported faithfully as a fetched-but-unrendered
 * seam (the decision is recorded in the status log).
 */

import { computed, signal, type Signal, type WritableSignal } from '@angular/core';

import type { CoreClient } from '../../core/core-client';
import type {
  CharacterListItem,
  ChatCreateDto,
  ScenarioDto,
  ScenarioListDto,
} from '../../core/core-contract';
import type { GreenRoomController } from './green-room.types';
import { buildCreateRequest } from './new-chat.logic';
import {
  INITIAL_FORM_STATE,
  type GroupScenarioOption,
  type NewChatFormState,
  type NewChatProject,
  type NewChatSelectedCharacter,
  type ProjectListEntry,
  type ScenarioOption,
} from './new-chat.types';

export interface NewChatOptions {
  initialCharacterId?: string;
  projectId?: string;
  /** Start the form in autonomous-room mode (v4 `?autonomous=1`). */
  initialAutonomous?: boolean;
}

/** A `{ chatId, isAutonomous } | null` submit outcome (v4 `handleCreateChat` return). */
export type CreateOutcome = { chatId: string; isAutonomous: boolean } | null;

function mapScenario(s: ScenarioDto): ScenarioOption {
  return {
    path: s.path,
    filename: s.filename,
    name: s.name,
    ...(s.description !== undefined && { description: s.description }),
    isDefault: s.isDefault,
    body: s.body,
  };
}

export class NewChatState {
  readonly loading = signal(true);
  readonly creating = signal(false);
  /** Surfaces load / pre-flight failures (v4 shows a toast; the SPA uses a banner). */
  readonly error = signal<string | null>(null);

  readonly characters = signal<CharacterListItem[]>([]);
  readonly profiles = signal<{ id: string; name: string; provider?: string; modelName?: string }[]>(
    [],
  );
  readonly userControlledCharacters = signal<CharacterListItem[]>([]);
  readonly project = signal<NewChatProject | null>(null);
  readonly projectScenarios = signal<ScenarioOption[]>([]);
  readonly generalScenarios = signal<ScenarioOption[]>([]);
  /** Fetched faithfully (participant union) but never rendered — dead UI in v4. */
  readonly groupScenarios = signal<GroupScenarioOption[]>([]);
  readonly availableProjects = signal<ProjectListEntry[]>([]);
  readonly selectedProjectId: WritableSignal<string | null>;

  readonly selectedCharacters = signal<NewChatSelectedCharacter[]>([]);
  readonly form = signal<NewChatFormState>({ ...INITIAL_FORM_STATE });

  /** The LLM cast (order-preserving), for the title, the outfit list, and validation. */
  readonly llmSelected = computed(() =>
    this.selectedCharacters().filter((sc) => sc.controlledBy === 'llm'),
  );

  /** Whether submit may proceed (v4's `canSubmit`, incl. the autonomous arm). */
  readonly canSubmit = computed(() => {
    const cast = this.selectedCharacters();
    const base =
      !this.creating() &&
      cast.length > 0 &&
      this.llmSelected().length > 0 &&
      !cast.some((sc) => sc.controlledBy === 'llm' && !sc.connectionProfileId) &&
      this.profiles().length > 0;
    if (!base) return false;
    // Autonomous rooms need ≥ 2 LLM characters and no user participant (v4).
    if (this.form().autonomous.enabled) {
      return this.llmSelected().length >= 2 && !cast.some((sc) => sc.controlledBy === 'user');
    }
    return true;
  });

  /** True once an initial seed ran (v4 `seededRef`) — gates the single-LLM propagation. */
  private seeded = false;
  private prevLlmIds = '';

  constructor(
    private readonly core: CoreClient,
    private readonly options: NewChatOptions,
    private readonly greenRoom: GreenRoomController | null = null,
  ) {
    this.selectedProjectId = signal<string | null>(options.projectId ?? null);
    if (options.initialAutonomous) {
      this.form.update((prev) => ({
        ...prev,
        autonomous: { ...prev.autonomous, enabled: true },
      }));
    }
  }

  // -------------------------------------------------------------------------
  // Load + seed
  // -------------------------------------------------------------------------

  /** The batched reference-data load + seeding (v4 `fetchData`). Re-runnable. */
  async load(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const projectId = this.selectedProjectId();
      const initialCharacterId = this.options.initialCharacterId;

      const [charsData, profilesList, generalData, projectListData] = await Promise.all([
        this.core.dispatchData({ type: 'characterList' }),
        this.core.dispatchExpect({ type: 'connectionProfileList' }, 'connectionProfiles'),
        this.core.dispatchData({ type: 'scenarioList' }),
        this.core.dispatchData({ type: 'projectList' }),
      ]);

      const all = (charsData['characters'] as CharacterListItem[]) ?? [];
      const loadedCharacters = all.filter((c) => c.controlledBy !== 'user');
      const loadedUserChars = all.filter((c) => c.controlledBy === 'user');
      const loadedProfiles = profilesList.data.profiles.map((p) => ({
        id: p.id,
        name: p.name,
        provider: p.provider,
        modelName: p.modelName,
      }));
      const loadedGeneral = ((generalData as unknown as ScenarioListDto).scenarios ?? []).map(
        mapScenario,
      );
      const loadedProjects: ProjectListEntry[] = (
        (projectListData['projects'] as { id: string; name: string; color?: string | null }[]) ?? []
      ).map((p) => ({ id: p.id, name: p.name, color: p.color ?? null }));

      // Project + its scenarios (defaults ride the detail GET, not the list).
      let loadedProject: NewChatProject | null = null;
      let loadedProjectScenarios: ScenarioOption[] = [];
      if (projectId) {
        const [projData, projScenData] = await Promise.all([
          this.core.dispatchData({ type: 'projectGet', projectId }),
          this.core.dispatchData({ type: 'projectScenarioList', projectId }),
        ]);
        loadedProject = (projData['project'] as NewChatProject) ?? null;
        loadedProjectScenarios = ((projScenData as unknown as ScenarioListDto).scenarios ?? []).map(
          mapScenario,
        );
      }

      // The seed character + its default partner (?characterId=).
      let seededChar: CharacterListItem | null = null;
      let seededPartnerId: string | null = null;
      if (initialCharacterId) {
        const [charDetail, partner] = await Promise.all([
          this.core.dispatchData({ type: 'characterGet', characterId: initialCharacterId }),
          this.core.dispatchData({
            type: 'characterDefaultPartner',
            characterId: initialCharacterId,
          }),
        ]);
        seededChar = (charDetail['character'] as CharacterListItem) ?? null;
        seededPartnerId = (partner['partnerId'] as string | null) ?? null;
      }

      this.characters.set(loadedCharacters);
      this.userControlledCharacters.set(loadedUserChars);
      this.profiles.set(loadedProfiles);
      this.project.set(loadedProject);
      this.projectScenarios.set(loadedProjectScenarios);
      this.generalScenarios.set(loadedGeneral);
      this.availableProjects.set(loadedProjects);

      const projectDefaultPath = loadedProjectScenarios.find((s) => s.isDefault)?.path ?? null;
      const generalDefaultPath = loadedGeneral.find((s) => s.isDefault)?.path ?? null;
      const seededGeneralPath = projectDefaultPath ? null : generalDefaultPath;

      if (initialCharacterId && seededChar && !this.seeded) {
        this.seeded = true;
        this.seedFromCharacter(
          seededChar,
          seededPartnerId,
          loadedProject,
          all,
          projectDefaultPath,
          seededGeneralPath,
        );
      } else if (loadedProject) {
        this.form.update((prev) => ({
          ...prev,
          projectScenarioPath: projectDefaultPath,
          generalScenarioPath: seededGeneralPath,
          imageProfileId: loadedProject.defaultImageProfileId || prev.imageProfileId,
          avatarGenerationEnabled:
            loadedProject.defaultAvatarGenerationEnabled ?? prev.avatarGenerationEnabled,
        }));
      } else if (generalDefaultPath) {
        this.form.update((prev) => ({ ...prev, generalScenarioPath: generalDefaultPath }));
      }

      // Faithful participant-union fetch (dead UI — never rendered).
      void this.refreshGroupScenarios();
    } catch (err) {
      this.error.set('Failed to load chat creation data');
      // eslint-disable-next-line no-console
      console.error('[NewChatState] load failed', err);
    } finally {
      this.loading.set(false);
    }
  }

  /** Seed the cast + form from a `?characterId=` character (v4's seed branch). */
  private seedFromCharacter(
    char: CharacterListItem,
    seededPartnerId: string | null,
    loadedProject: NewChatProject | null,
    allCharacters: CharacterListItem[],
    projectDefaultPath: string | null,
    seededGeneralPath: string | null,
  ): void {
    const connectionProfileId = char.defaultConnectionProfileId || this.profiles()[0]?.id || '';
    const prompts = char.systemPrompts ?? [];
    const defaultPromptId = char.defaultSystemPromptId
      ? prompts.find((p) => p.id === char.defaultSystemPromptId)?.id
      : (prompts.find((p) => p.isDefault)?.id ?? prompts[0]?.id);
    const seeded: NewChatSelectedCharacter[] = [
      {
        character: char,
        connectionProfileId,
        selectedSystemPromptId: defaultPromptId ?? null,
        controlledBy: 'llm',
      },
    ];
    const partnerId = seededPartnerId || char.defaultPartnerId || '';
    if (partnerId) {
      const partner = allCharacters.find((c) => c.id === partnerId);
      if (partner && partner.id !== char.id) {
        seeded.push({
          character: partner,
          connectionProfileId: '',
          selectedSystemPromptId: null,
          controlledBy: 'user',
        });
      }
    }
    this.selectedCharacters.set(seeded);
    this.prevLlmIds = [char.id].sort().join(',');
    this.form.update((prev) => ({
      ...prev,
      timestampConfig: char.defaultTimestampConfig ?? null,
      scenarioId: char.defaultScenarioId ?? null,
      projectScenarioPath: projectDefaultPath,
      generalScenarioPath: seededGeneralPath,
      imageProfileId: loadedProject?.defaultImageProfileId || char.defaultImageProfileId || '',
      avatarGenerationEnabled: loadedProject?.defaultAvatarGenerationEnabled ?? false,
    }));
  }

  /** Re-fetch the group participant-union when a project is chosen (v4 full refetch). */
  async setSelectedProjectId(id: string | null): Promise<void> {
    this.selectedProjectId.set(id);
    await this.load();
  }

  private async refreshGroupScenarios(): Promise<void> {
    const characterIds = this.llmSelected().map((sc) => sc.character.id);
    if (characterIds.length === 0) {
      this.groupScenarios.set([]);
      return;
    }
    try {
      const data = await this.core.dispatchData({ type: 'groupScenariosUnion', characterIds });
      const groups =
        (data['groupScenarios'] as Array<{
          groupId: string;
          groupName: string;
          scenarios: ScenarioDto[];
        }>) ?? [];
      const flat: GroupScenarioOption[] = [];
      for (const g of groups) {
        for (const s of g.scenarios ?? []) {
          flat.push({ ...mapScenario(s), groupId: g.groupId, groupName: g.groupName });
        }
      }
      this.groupScenarios.set(flat);
    } catch {
      // Dead UI — a union failure is non-fatal.
      this.groupScenarios.set([]);
    }
  }

  // -------------------------------------------------------------------------
  // Cast mutations (drive the single-LLM default propagation)
  // -------------------------------------------------------------------------

  /**
   * Replace the cast, then run the single-LLM default propagation (v4's
   * post-load effect): when exactly one LLM character is selected and no initial
   * seed ran, propagate its defaults and auto-join its default partner as the
   * user persona.
   */
  setSelectedCharacters(
    updater: (prev: NewChatSelectedCharacter[]) => NewChatSelectedCharacter[],
  ): void {
    this.selectedCharacters.update(updater);
    this.syncSingleLlmDefaults();
    void this.refreshGroupScenarios();
  }

  private syncSingleLlmDefaults(): void {
    const llm = this.llmSelected();
    const currentIds = llm
      .map((sc) => sc.character.id)
      .sort()
      .join(',');
    if (currentIds === this.prevLlmIds) return;
    this.prevLlmIds = currentIds;

    if (llm.length === 1 && !this.seeded) {
      const char = llm[0].character;
      this.form.update((prev) => ({
        ...prev,
        timestampConfig: char.defaultTimestampConfig ?? prev.timestampConfig,
        scenarioId: char.defaultScenarioId ?? prev.scenarioId,
        imageProfileId:
          this.project()?.defaultImageProfileId ||
          char.defaultImageProfileId ||
          prev.imageProfileId,
      }));
      const partnerId = char.defaultPartnerId;
      if (partnerId) {
        this.selectedCharacters.update((prev) => {
          if (prev.some((sc) => sc.controlledBy === 'user')) return prev;
          if (prev.some((sc) => sc.character.id === partnerId)) return prev;
          const partner = this.userControlledCharacters().find((c) => c.id === partnerId);
          if (!partner) return prev;
          return [
            ...prev,
            {
              character: partner,
              connectionProfileId: '',
              selectedSystemPromptId: null,
              controlledBy: 'user',
            },
          ];
        });
      }
    }
  }

  patchForm(patch: Partial<NewChatFormState>): void {
    this.form.update((prev) => ({ ...prev, ...patch }));
  }

  // -------------------------------------------------------------------------
  // Submit
  // -------------------------------------------------------------------------

  /**
   * The submit spine (v4 `handleCreateChat`): pre-flight validation → generate a
   * `progressId` and open the Green Room BEFORE the dispatch → dispatch → the
   * dispatch resolving is the authoritative ready signal (navigate, then close
   * the dialog); a failure surfaces in the dialog with a Close button.
   */
  async handleCreate(): Promise<CreateOutcome> {
    const cast = this.selectedCharacters();
    if (cast.length === 0) {
      this.error.set('Please select at least one character');
      return null;
    }
    const isAutonomous = this.form().autonomous.enabled;
    if (isAutonomous) {
      if (this.llmSelected().length < 2) {
        this.error.set('Autonomous rooms need at least two LLM-controlled characters');
        return null;
      }
      if (cast.some((sc) => sc.controlledBy === 'user')) {
        this.error.set('Autonomous rooms have no user — remove user-controlled characters');
        return null;
      }
    }
    const missingProfile = cast.filter(
      (sc) => sc.controlledBy === 'llm' && !sc.connectionProfileId,
    );
    if (missingProfile.length > 0) {
      this.error.set(
        `Please select a connection profile for: ${missingProfile.map((sc) => sc.character.name).join(', ')}`,
      );
      return null;
    }
    if (!cast.some((sc) => sc.controlledBy === 'llm')) {
      this.error.set('At least one character must be LLM-controlled');
      return null;
    }

    this.error.set(null);
    this.creating.set(true);

    const progressId =
      this.greenRoom && typeof crypto !== 'undefined' && crypto.randomUUID
        ? crypto.randomUUID()
        : undefined;

    try {
      const body = buildCreateRequest(cast, this.form(), this.selectedProjectId(), progressId);
      if (progressId) this.greenRoom?.begin(progressId);
      const resp = await this.core.dispatchExpect(body, 'chatCreate');
      const chatId = (resp.data as ChatCreateDto).chat.id;
      this.greenRoom?.complete();
      return { chatId, isAutonomous };
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to create chat';
      this.error.set(msg);
      if (progressId) this.greenRoom?.fail(msg);
      return null;
    } finally {
      this.creating.set(false);
    }
  }
}

/** Narrowed read view the form/picker consume (keeps the writable signals internal). */
export interface NewChatStateView {
  readonly selectedCharacters: Signal<NewChatSelectedCharacter[]>;
  readonly form: Signal<NewChatFormState>;
}
