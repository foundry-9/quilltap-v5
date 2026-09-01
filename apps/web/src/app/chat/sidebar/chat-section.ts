import {
  afterRenderEffect,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { toggleAgentMode } from '../chat-admin.api';
import { toggleAvatarGeneration } from '../chat-cast.api';
import { CoreClient } from '../../core/core-client';
import type {
  ApiKeyDto,
  ConciergeState as ConciergeStateWire,
  ImageProfileDto,
  RoleplayTemplateDto,
} from '../../core/core-contract';
import {
  getConciergeState,
  type ConciergeOverrideValue,
} from '../concierge-state';
import { fetchImageProfiles, imageProfileKeys } from '../../screens/settings/images/image-profiles.api';
import { fetchRoleplayTemplates, templateKeys } from '../../screens/settings/templates/templates.api';
import { Icon, type IconName } from '../../ui/icon';
import { ToastService } from '../../ui/toast.service';
import { ChatScenarioControl } from './chat-scenario-control';

/** The chat-record fields this section edits. `null` ⇒ not set / inherit. */
/** v4's four success sentences (`ChatSidebar.tsx:1080-1088`), byte for byte. */
const CONCIERGE_TOASTS: Record<ConciergeStateWire, string> = {
  monitored: 'The Concierge is on watch',
  flagged: 'Marked as flagged',
  vouched: 'You have vouched for this chat',
  uncensored: 'The uncensored door stands open',
};

export interface ChatSectionState {
  roleplayTemplateId: string | null;
  /** v4 `chats.avatarGenerationEnabled` — projected by the route (get.ts:558). */
  avatarGenerationEnabled: boolean | null;
  timelineMode: 'realtime' | 'narrative' | null;
  imageProfileId: string | null;
  alertCharactersOfLanternImages: boolean | null;
  projectId: string | null;
  projectName: string | null;
  /**
   * The scene this chat runs on (v4 `44a8137e`). v4 threads it to `ChatSection`
   * as its own prop alongside `projectId`; v5's sidebar carries the whole slice
   * in this bag, which `projectId` already rode, so it joins it here.
   */
  scenarioText: string | null;
  /**
   * The RESOLVED agent-mode value (v4 syncs its local `agentModeEnabled` from
   * `chat.resolvedAgentModeEnabled`, `useChatControls.ts:76-82`), which is what
   * the badge displays and what its next-value computation reads.
   */
  agentModeEnabled: boolean | null;
}

/**
 * The sidebar's "Chat" section (v4 `ChatSidebar.tsx:872-1270` `ChatSection`):
 * the per-chat selects, each writing straight through `PUT /chats/{id}` and
 * telling the parent to refetch — and, since v4 `8bf3cb5f`, **the Story's
 * Clock**, the episodic timeline-mode switch this whole round exists for.
 *
 * Reference data (templates / image profiles / API keys) is fetched only once
 * the section has been opened, exactly as v4 latches `hasEverOpened` — a user
 * who never expands this accordion never hits those endpoints.
 *
 * ## Two things worth knowing before reading the selects
 *
 * **1. The controlled selects survive a reload.** Each is a local signal seeded
 * from the prop and re-synced when the prop moves — v4's OWN idiom for the
 * sibling control on the same panel (`selectedTemplateId` + its re-sync effect,
 * `ChatSidebar.tsx:892/901-904`). Historically v4's chat GET forgot to project
 * `timelineMode`, `alertCharactersOfLanternImages`, `showThinking` and
 * `answerConfirmationOverride`, so those props arrived `undefined`, the seed was
 * a no-op, and each select snapped back to its default the moment a save
 * re-rendered — the write landed, the display did not (v5 carried this as a
 * deliberate reload divergence). v4 `bd419ae9` (bug 22) added all four to the
 * projection; v5's server mirrors it (P4.D53 §1). The seed now finds a real
 * value, so a reload shows the saved choice — the divergence is retired.
 *
 * **2. The outcomes are toasts**, v4's own (`ChatSidebar.tsx:950-1081`), with
 * v4's copy verbatim.
 *
 * **3. Every write goes through the `chat` bag.** v4 posts the template /
 * image-profile / announce writes as TOP-LEVEL keys (`{roleplayTemplateId}`)
 * and only the Story's Clock as `{chat: {timelineMode}}`; both shapes land on
 * the same columns via `updateChatSchema`. v5's `chatUpdate` carries the `chat`
 * bag plus the named siblings v4's own schema declares at the top level — the
 * REMAINING top-level shortcuts are a server-side deferral (`api/salon.rs`) —
 * so the client uses the bag form for everything else, which is the shape the
 * round-2 differential pins.
 *
 * **The Concierge four-state is LIVE** (v4 :1123-1170, P4.D141) — the control
 * this section led with as a named deferral for six rounds. `conciergeState` is
 * a SIBLING of the `chat` bag, not a bag key: v4's `chatUpdateRequestSchema`
 * declares it at the top level and `processChatUpdates` routes it through
 * `applyConciergeFlip`. Sent as a bag key it would be stripped by
 * `updateChatSchema` and silently do nothing.
 *
 * **Run Tool… is LIVE** (v4 :1243-1255, P4.9E3C) — an operator-initiated call over
 * the same inventory. Not to be confused with v5's composer custom-tools popup,
 * which is Pascal's surface and a different thing.
 *
 * **Tools… is LIVE** (v4 :1230-1241, P4.9E3C) over P4.9E3B's tool inventory. The
 * entry REFUSED BY NAME until then, the inventory (`GET /api/v1/tools`, 727 LOC)
 * having been unported.
 *
 * **Agent Mode is LIVE** (v4 :1116-1127) — {@link ChatSection.onAgentModeToggle}.
 * It had been tracked in NEITHER of `m6-screen-parity.md`'s two tables, being a
 * badge rather than a modal, until the 2026-07-27 dogfood walk went looking for
 * it (finding #34).
 *
 * **Project is LIVE** (v4 :1166-1177, P4.9E3C 2026-07-27): the entry opens
 * `ChatProjectModal` and is shown unconditionally, so a chat can be filed as well
 * as re-filed. It was REDUCED until then — a link to the Prospero screen, rendered
 * only when the chat already HAD a project, which meant an unfiled chat could
 * never be filed from the Salon at all.
 */
@Component({
  selector: 'qt-chat-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ChatScenarioControl, Icon],
  template: `
    <div class="qt-chat-sidebar-section qt-chat-sidebar-section-chat flex flex-col gap-3">
      <!--
        The Concierge — per-chat four-state (v4 ChatSidebar.tsx:1146-1170).
        Four states, one 2x2: rows are the route (ordinary vs uncensored),
        columns are the provenance (the Concierge's classifier vs the operator).
        The optgroups carry the provenance structurally; the helper text names
        the actor; the icon/color pair gives a third, colorblind-safe channel.
      -->
      <label class="qt-label">
        <span class="mb-1 flex items-center gap-1.5">
          The Concierge
          <qt-icon
            [name]="conciergeStateIcon().name"
            [class]="'w-3.5 h-3.5 ' + conciergeStateIcon().className"
          />
        </span>
        <select
          #conciergeSelect
          class="qt-select text-sm"
          [value]="conciergeState()"
          [disabled]="conciergeSaving()"
          (change)="onConciergeStateChange($event)"
        >
          <optgroup label="The Concierge decides">
            <option value="monitored">Monitored</option>
            <option value="flagged">Flagged</option>
          </optgroup>
          <optgroup label="You decide">
            <option value="vouched">Vouched Safe</option>
            <option value="uncensored">Uncensored</option>
          </optgroup>
        </select>
        <span class="block mt-1 qt-text-secondary text-xs">{{ conciergeHelperText() }}</span>
      </label>

      <!-- Agent Mode (v4 :1116-1127), which v4 places directly after the
           Concierge control. -->
      <button
        type="button"
        [class]="
          'qt-tool-palette-badge ' +
          (agentModeOn() ? 'qt-tool-palette-badge-on' : 'qt-tool-palette-badge-off')
        "
        [title]="agentModeOn() ? 'Disable agent mode' : 'Enable agent mode'"
        [disabled]="agentModeSaving()"
        (click)="onAgentModeToggle()"
      >
        <qt-icon name="monitor" class="w-3.5 h-3.5" />
        <span>{{ agentModeOn() ? 'Agent On' : 'Agent Off' }}</span>
      </button>

      <!-- Roleplay Template -->
      <label class="qt-label">
        <span class="block mb-1">Roleplay Template</span>
        <select
          class="qt-select text-sm"
          [value]="selectedTemplateId() ?? ''"
          [disabled]="templateSaving()"
          (change)="onTemplateChange($event)"
        >
          <option value="">No Template</option>
          @for (template of roleplayTemplates(); track template.id) {
            <option [value]="template.id" [selected]="template.id === selectedTemplateId()">
              {{ template.name }}{{ template.isBuiltIn ? ' (Built-in)' : '' }}
            </option>
          }
        </select>
      </label>

      <!-- Scenario — the scene this chat runs on (v4 44a8137e, in v4's slot) -->
      <qt-chat-scenario-control
        [chatId]="chatId()"
        [projectId]="state().projectId"
        [scenarioText]="state().scenarioText"
        [llmCharacterIds]="llmCharacterIds()"
        [singleLlmCharacterId]="singleLlmCharacterId()"
        [enabled]="hasEverOpened()"
        (chatUpdated)="chatUpdated.emit()"
      />

      <!-- The Story's Clock — episodic-memory timeline mode -->
      <label class="qt-label">
        <span class="block mb-1">The Story&rsquo;s Clock</span>
        <select
          class="qt-select text-sm"
          [value]="timelineValue()"
          [disabled]="timelineModeSaving()"
          (change)="onTimelineModeChange($event)"
        >
          <option value="realtime" [selected]="timelineValue() === 'realtime'">Real time</option>
          <option value="narrative" [selected]="timelineValue() === 'narrative'">Story time</option>
        </select>
        <span class="block mt-1 qt-text-secondary text-xs">
          @if (timelineValue() === 'narrative') {
            The tale keeps its own hours: the Commonplace Book files memories by the story’s
            reckoning — the third night at sea, the eve of the coronation — rather than the calendar
            on the wall.
          } @else {
            Memories are filed by the calendar on the wall — “last Tuesday” means the actual
            Tuesday. Switch to Story time if this chat runs on a fictional clock.
          }
        </span>
      </label>

      <!-- Project (v4 :1166-1177) -->
      <button
        type="button"
        class="qt-tool-palette-button"
        [title]="
          state().projectName ? 'In project: ' + state().projectName : 'Assign to project'
        "
        (click)="openProject.emit()"
      >
        <qt-icon name="folder" class="w-4 h-4" />
        <span>{{ state().projectName ? 'Project: ' + state().projectName : 'Project' }}</span>
      </button>

      <!-- Image Provider -->
      <label class="qt-label">
        <span class="block mb-1">Image Provider</span>
        <select
          class="qt-select text-sm"
          [value]="selectedImageProfileId() ?? ''"
          [disabled]="imageProfileSaving()"
          (change)="onImageProfileChange($event)"
        >
          <option value="">None (image generation disabled)</option>
          @for (profile of imageProfiles(); track profile.id) {
            <option [value]="profile.id" [selected]="profile.id === selectedImageProfileId()">
              {{ profile.name }} ({{ profile.provider }}){{ missingKeySuffix(profile) }}
            </option>
          }
        </select>
      </label>

      <!-- Announce Generated Images -->
      <label class="qt-label">
        <span class="block mb-1">Announce Generated Images</span>
        <select
          class="qt-select text-sm"
          [value]="alertSelectValue()"
          [disabled]="alertImagesSaving()"
          (change)="onAlertImagesChange($event)"
        >
          <option value="inherit" [selected]="alertSelectValue() === 'inherit'">
            Inherit from project
          </option>
          <option value="enabled" [selected]="alertSelectValue() === 'enabled'">
            Announce to characters
          </option>
          <option value="disabled" [selected]="alertSelectValue() === 'disabled'">
            Keep silent
          </option>
        </select>
      </label>

      <!-- Auto-generate Character Avatars (v4 :1218-1228) -->
      <label class="flex items-center gap-2 cursor-pointer">
        <input
          type="checkbox"
          class="qt-checkbox"
          aria-label="Auto-generate character avatars"
          [checked]="avatarGenerationEnabled()"
          [disabled]="avatarGenSaving()"
          (change)="onAvatarGenToggle()"
        />
        <span class="qt-label">Auto-generate character avatars</span>
      </label>

      <!-- Run Tool… (v4 :1243-1255) -->
      <button
        type="button"
        class="qt-tool-palette-button"
        title="Run a tool manually"
        (click)="openRunTool.emit()"
      >
        <qt-icon name="wrench" class="w-4 h-4" />
        <span>Run Tool…</span>
      </button>

      <!-- Tools… (v4 :1230-1241) -->
      <button
        type="button"
        class="qt-tool-palette-button"
        title="Configure LLM tools"
        (click)="openToolSettings.emit()"
      >
        <qt-icon name="settings" class="w-4 h-4" />
        <span>Tools…</span>
      </button>

      <!-- Regenerate Story Background -->
      @if (storyBackgroundsEnabled()) {
        <button
          type="button"
          class="qt-tool-palette-button"
          title="Regenerate story background image"
          [disabled]="regeneratingBackground()"
          (click)="regenerateBackground.emit()"
        >
          <qt-icon name="image" class="w-4 h-4" />
          <span>Regenerate Background</span>
        </button>
      }
    </div>
  `,
})
export class ChatSection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();
  readonly state = input.required<ChatSectionState>();
  /** v4 `chatControls.storyBackgroundsEnabled` — gates the regenerate entry. */
  readonly storyBackgroundsEnabled = input(false);
  /** Disables the regenerate entry while a poll is in flight (v5's guard). */
  readonly regeneratingBackground = input(false);
  /** True while this accordion section is open — latches the reference fetches. */
  readonly sectionOpen = input(false);
  /** Character IDs of the LLM-controlled cast, for the scenario picker's group tier. */
  readonly llmCharacterIds = input<readonly string[]>([]);
  /** The lone LLM character's ID, or null when several share the room. */
  readonly singleLlmCharacterId = input<string | null>(null);
  /**
   * The Concierge's two stored fields (P4.D141), mirroring v4's own
   * `ChatSidebarProps` — which carries `isDangerousChat` and `conciergeOverride`
   * as siblings for exactly this reason. They are meaningful only read TOGETHER
   * (see `chat/concierge-state.ts`), so they arrive as a pair rather than
   * through the `ChatSectionState` bag.
   *
   * ⚠ **`conciergeOverride` needs a cross-lane wire.** Its value originates in
   * `screens/salon/salon-conversation.ts`, which **P4.66 owns**, so P4.D141 may
   * not add the binding. Until the unifier does (one line — see this lane's
   * record), the input defaults to `null` and the control can show only
   * Monitored / Flagged: it will never DISPLAY an operator state, though writing
   * one works end to end. This is recorded loudly rather than hidden behind a
   * silent default.
   */
  readonly isDangerousChat = input<boolean | null>(null);
  readonly conciergeOverride = input<ConciergeOverrideValue | null>(null);

  /** Fired after any chat-record field is mutated (v4 `onChatUpdated` → fetchChat). */
  readonly chatUpdated = output<void>();
  readonly regenerateBackground = output<void>();
  /** v4 `onProjectClick` — opens `ChatProjectModal`. */
  readonly openProject = output<void>();
  /** v4 `onToolSettingsClick` — opens `ChatToolSettingsModal`. */
  readonly openToolSettings = output<void>();
  /** v4 `onRunToolClick` — opens `RunToolModal`. */
  readonly openRunTool = output<void>();

  /**
   * v4 `handleAvatarGenToggle` (`ChatSidebar.tsx:1065-1083`): POST the toggle,
   * read `avatarGenerationEnabled` off the reply for the toast, and tell the
   * parent to refetch. The checkbox reads the chat's own projected value, so
   * the refetch is what makes it stick.
   */
  protected readonly avatarGenerationEnabled = computed(
    () => this.state().avatarGenerationEnabled === true,
  );
  protected readonly avatarGenSaving = signal(false);

  /**
   * The badge's displayed value. Seeded from the chat's RESOLVED agent mode and
   * re-synced when it moves (v4 `useChatControls.ts:76-82`), then adopted
   * optimistically from the reply — which is what makes the badge flip without
   * waiting for the parent refetch.
   */
  private readonly localAgentMode = signal<boolean | null>(null);
  protected readonly agentModeOn = computed(() => this.localAgentMode() === true);
  protected readonly agentModeSaving = signal(false);

  /**
   * v4 `handleToggleAgentMode`: compute the next value from the CURRENT one,
   * post it, adopt `resolvedAgentModeEnabled ?? agentModeEnabled` off the reply,
   * and report the outcome. A failure leaves the badge where it was — v4 never
   * flips optimistically here, so neither does this.
   */
  protected async onAgentModeToggle(): Promise<void> {
    this.agentModeSaving.set(true);
    try {
      const result = await toggleAgentMode(this.core, this.chatId(), this.localAgentMode());
      this.localAgentMode.set(result.enabled);
      this.toasts.showSuccess(`Agent mode ${result.status}`);
    } catch (error) {
      this.toasts.showError(
        error instanceof Error ? error.message : 'Failed to toggle agent mode',
      );
    } finally {
      this.agentModeSaving.set(false);
    }
  }

  protected async onAvatarGenToggle(): Promise<void> {
    this.avatarGenSaving.set(true);
    try {
      const enabled = await toggleAvatarGeneration(this.core, this.chatId());
      this.toasts.showSuccess(
        enabled === false ? 'Avatar generation disabled' : 'Avatar generation enabled',
      );
      this.chatUpdated.emit();
    } catch (err) {
      this.toasts.showError(
        err instanceof Error ? err.message : 'Failed to toggle avatar generation',
      );
    } finally {
      this.avatarGenSaving.set(false);
    }
  }

  protected readonly templateSaving = signal(false);
  protected readonly imageProfileSaving = signal(false);
  protected readonly alertImagesSaving = signal(false);
  protected readonly timelineModeSaving = signal(false);

  /**
   * The operator's current choice per control. Seeded from the chat record and
   * re-synced whenever it moves upstream (v4's `selectedTemplateId` effect); for
   * the columns the GET never returns, this is what keeps the select from
   * snapping back after a successful write. See the class doc.
   */
  private readonly localTemplateId = signal<string | null>(null);
  private readonly localImageProfileId = signal<string | null>(null);
  private readonly localTimelineMode = signal<'realtime' | 'narrative' | null>(null);
  private readonly localAlertImages = signal<boolean | null | undefined>(undefined);

  /** One-shot latch: fetch reference data only after the section is first opened. */
  protected readonly hasEverOpened = signal(false);

  constructor() {
    // React re-applies a controlled <select>'s value on every render, so v4's
    // control can never sit on a pick the stored state does not carry (a
    // rejected write, a refetch, an auto-flip, another tab). Angular's [value]
    // writes only when the BOUND value changes, which would let the user's pick
    // stand until the next change — so the element is re-written from the
    // derived state after each render instead (the P4.D115 ScenarioSelect idiom).
    afterRenderEffect(() => {
      const value = this.conciergeState();
      this.conciergeSaving();
      const select = this.conciergeSelectRef()?.nativeElement;
      if (select && select.value !== value) select.value = value;
    });
    effect(() => {
      if (this.sectionOpen() && !this.hasEverOpened()) {
        this.hasEverOpened.set(true);
      }
    });
    // v4 `useEffect(() => setSelectedTemplateId(roleplayTemplateId ?? null), [roleplayTemplateId])`,
    // generalized to the whole write-through set.
    effect(() => {
      const s = this.state();
      this.localTemplateId.set(s.roleplayTemplateId ?? null);
      this.localImageProfileId.set(s.imageProfileId ?? null);
      this.localTimelineMode.set(s.timelineMode ?? null);
      this.localAlertImages.set(s.alertCharactersOfLanternImages);
      this.localAgentMode.set(s.agentModeEnabled);
    });
  }

  private readonly templatesQuery = injectQuery(() => ({
    queryKey: templateKeys.list(),
    queryFn: (): Promise<RoleplayTemplateDto[]> => fetchRoleplayTemplates(this.core),
    enabled: this.hasEverOpened(),
  }));
  private readonly imageProfilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: (): Promise<ImageProfileDto[]> => fetchImageProfiles(this.core),
    enabled: this.hasEverOpened(),
  }));
  private readonly apiKeysQuery = injectQuery(() => ({
    queryKey: ['api-keys'],
    queryFn: async (): Promise<ApiKeyDto[]> => {
      const resp = await this.core.dispatchExpect({ type: 'apiKeyList' }, 'apiKeys');
      return resp.data.apiKeys;
    },
    enabled: this.hasEverOpened(),
  }));

  protected readonly roleplayTemplates = computed(() => this.templatesQuery.data() ?? []);
  protected readonly imageProfiles = computed(() => this.imageProfilesQuery.data() ?? []);
  private readonly apiKeys = computed(() => this.apiKeysQuery.data() ?? []);

  protected readonly selectedTemplateId = computed(() => this.localTemplateId());
  protected readonly selectedImageProfileId = computed(() => this.localImageProfileId());
  /** v4 `timelineMode === 'narrative' ? 'narrative' : 'realtime'` — null reads realtime. */
  protected readonly timelineValue = computed(() =>
    this.localTimelineMode() === 'narrative' ? 'narrative' : 'realtime',
  );
  /** v4 `alertSelectValue` — null/undefined ⇒ inherit. */
  protected readonly alertSelectValue = computed(() => {
    const v = this.localAlertImages();
    if (v === null || v === undefined) return 'inherit';
    return v ? 'enabled' : 'disabled';
  });

  /** v4 `{!hasKey && profile.apiKeyId ? ' ⚠️ No API Key' : ''}`. */
  protected missingKeySuffix(profile: ImageProfileDto): string {
    const hasKey = this.apiKeys().some((key) => key.id === profile.apiKeyId);
    return !hasKey && profile.apiKeyId ? ' ⚠️ No API Key' : '';
  }

  protected onTemplateChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const value = select.value || null;
    const previous = this.localTemplateId();
    this.localTemplateId.set(value);
    void this.write(
      { roleplayTemplateId: value },
      this.templateSaving,
      'Roleplay template updated',
      'Failed to update roleplay template',
      () => {
        this.localTemplateId.set(previous);
        select.value = previous ?? '';
      },
    );
  }

  protected onImageProfileChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const value = select.value || null;
    const previous = this.localImageProfileId();
    this.localImageProfileId.set(value);
    void this.write(
      { imageProfileId: value },
      this.imageProfileSaving,
      'Image profile updated',
      'Failed to update image profile',
      () => {
        this.localImageProfileId.set(previous);
        select.value = previous ?? '';
      },
    );
  }

  protected onAlertImagesChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const raw = select.value;
    const value = raw === 'inherit' ? null : raw === 'enabled';
    const previous = this.localAlertImages();
    this.localAlertImages.set(value);
    void this.write(
      { alertCharactersOfLanternImages: value },
      this.alertImagesSaving,
      value === null
        ? 'Lantern announcements will inherit from the project'
        : value
          ? 'Lantern images will be announced to characters'
          : 'Lantern images will stay silent for this chat',
      'Failed to update Lantern image announcements',
      () => {
        this.localAlertImages.set(previous);
        select.value = previous == null ? 'inherit' : previous ? 'enabled' : 'disabled';
      },
    );
  }

  /**
   * Which clock the chat's story keeps. Persisted as the explicit enum value;
   * NULL in the record reads as 'realtime' (v4 `handleTimelineModeChange`).
   */
  protected onTimelineModeChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const value = select.value === 'narrative' ? 'narrative' : 'realtime';
    const previous = this.localTimelineMode();
    this.localTimelineMode.set(value);
    void this.write(
      { timelineMode: value },
      this.timelineModeSaving,
      value === 'narrative'
        ? 'The story now keeps its own hours'
        : 'The story is back on the clock on the wall',
      "Failed to change the story's clock",
      () => {
        this.localTimelineMode.set(previous);
        select.value = previous === 'narrative' ? 'narrative' : 'realtime';
      },
    );
  }

  // ── The Concierge four-state (v4 `ChatSidebar.tsx:1123-1170`, P4.D141) ──

  /**
   * The displayed state — derived from the chat's two stored fields through the
   * SHARED predicate and from NOTHING else, exactly as v4's `ChatSidebar.tsx:1123`
   * computes `getConciergeState({isDangerousChat, conciergeOverride})` straight
   * from props. There is deliberately no optimistic latch: a local "last pick"
   * that outlived the write would ignore a refetch, a classifier auto-flip and
   * another tab's change for the rest of the session, and let this control
   * disagree with the header badge (the P4.D141 unification review's catch).
   * The pick shows once the parent's refetch reflects it; a failed write needs
   * no revert because the element is re-written from this value after every
   * render (see the constructor's `afterRenderEffect`).
   */
  protected readonly conciergeState = computed<ConciergeStateWire>(() =>
    getConciergeState({
      isDangerousChat: this.isDangerousChat(),
      conciergeOverride: this.conciergeOverride(),
    }),
  );
  protected readonly conciergeSaving = signal(false);
  private readonly conciergeSelectRef = viewChild<ElementRef<HTMLSelectElement>>('conciergeSelect');

  /** v4's helper text, byte for byte — it names the ACTOR, not the effect. */
  protected readonly conciergeHelperText = computed(() => {
    switch (this.conciergeState()) {
      case 'monitored':
        return 'The Concierge keeps watch, and will flip the switch himself if the conversation calls for it.';
      case 'flagged':
        return 'The Concierge has this chat down as dangerous, and routes it through the uncensored providers.';
      case 'vouched':
        return 'You have vouched for this chat. The Concierge stops watching; the ordinary providers still apply, and may still refuse.';
      default:
        return 'You have sent the Concierge away and opened the uncensored door yourself. Nothing is scanned, nothing is softened — the risk is yours.';
    }
  });

  /** The third, colorblind-safe channel (v4 `conciergeStateIcon`). */
  protected readonly conciergeStateIcon = computed<{ name: IconName; className: string }>(() => {
    switch (this.conciergeState()) {
      case 'monitored':
        return { name: 'eye', className: 'qt-text-success' };
      case 'flagged':
        return { name: 'alert-triangle', className: 'qt-text-danger' };
      case 'vouched':
        return { name: 'check-circle', className: 'qt-text-muted' };
      default:
        return { name: 'eye-off', className: 'qt-text-info' };
    }
  });

  /**
   * v4 `handleConciergeStateChange` (`ChatSidebar.tsx:1068-1095`): PUT
   * `{conciergeState: next}` — a SIBLING of the `chat` bag, not a bag key —
   * then the state's own success copy and a parent refetch. v4 fires the PUT
   * unconditionally (no "already there" short-circuit) and holds no local
   * state: React re-applies the controlled `value` from props on every render,
   * so the element snaps to the STORED state until the refetch carries the
   * pick. v5 reproduces that re-apply with the constructor's
   * `afterRenderEffect` — which is also what reverts a rejected pick, since
   * the stored state never moved.
   */
  protected async onConciergeStateChange(event: Event): Promise<void> {
    const el = event.target as HTMLSelectElement;
    const next = el.value as ConciergeStateWire;
    this.conciergeSaving.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'chatUpdate',
        chatId: this.chatId(),
        chat: {},
        conciergeState: next,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to change the Concierge state');
      }
      this.toasts.showSuccess(CONCIERGE_TOASTS[next]);
      this.chatUpdated.emit();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      this.toasts.showError(msg || 'Failed to change the Concierge state');
    } finally {
      this.conciergeSaving.set(false);
    }
  }

  /**
   * One `PUT /api/v1/chats/{id}` with the given bag, then v4's post-write
   * ritual: the success copy and a parent refetch. The caller has already
   * adopted the new value optimistically (so the select shows what was picked
   * even for the columns the GET never returns — see the class doc); a failure
   * reverts it AND restores the `<select>`'s own value, which is what puts v4's
   * snapped-back control back on screen. (The element write is not redundant:
   * change detection never ran while the optimistic value was in the model, so
   * reverting the signal alone leaves the binding memo unchanged and the DOM
   * showing the rejected choice.)
   */
  private async write(
    bag: Record<string, unknown>,
    saving: ReturnType<typeof signal<boolean>>,
    successMessage: string,
    fallbackError: string,
    revert: () => void,
  ): Promise<void> {
    saving.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'chatUpdate',
        chatId: this.chatId(),
        chat: bag,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || fallbackError);
      }
      this.toasts.showSuccess(successMessage);
      this.chatUpdated.emit();
    } catch (error) {
      revert();
      const msg = error instanceof Error ? error.message : String(error);
      this.toasts.showError(msg || fallbackError);
    } finally {
      saving.set(false);
    }
  }
}
