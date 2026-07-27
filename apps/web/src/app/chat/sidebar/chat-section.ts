import { ChangeDetectionStrategy, Component, computed, effect, inject, input, output, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { toggleAgentMode } from '../chat-admin.api';
import { toggleAvatarGeneration } from '../chat-cast.api';
import { CoreClient } from '../../core/core-client';
import type { ApiKeyDto, ImageProfileDto, RoleplayTemplateDto } from '../../core/core-contract';
import { fetchImageProfiles, imageProfileKeys } from '../../screens/settings/images/image-profiles.api';
import { fetchRoleplayTemplates, templateKeys } from '../../screens/settings/templates/templates.api';
import { Icon } from '../../ui/icon';

/** The chat-record fields this section edits. `null` ⇒ not set / inherit. */
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
 * **1. v4's chat GET does not return four of these columns.** The route
 * (`app/api/v1/chats/[id]/handlers/get.ts:528-568`) projects an explicit object,
 * and `timelineMode`, `alertCharactersOfLanternImages`, `showThinking` and
 * `answerConfirmationOverride` are not in it — even though v4's own `Chat` type
 * declares them (`app/salon/[id]/types.ts:253-262`). So in v4 those props are
 * permanently `undefined` and their controlled selects snap back to the default
 * option the moment a save re-renders: the write lands, the display does not.
 * v5's server ports v4's projection faithfully (`api/salon.rs:303-422`), so the
 * same fields are missing here, and this lane may not change the server.
 *
 * **The one deliberate divergence:** every write-only select keeps the value the
 * operator just chose (a local signal seeded from the prop and re-synced when the
 * prop moves). That is not an invention — it is v4's OWN idiom for the sibling
 * control on the same panel (`selectedTemplateId` + its re-sync effect,
 * `ChatSidebar.tsx:892/901-904`), applied to the controls whose prop the route
 * forgets to send. A reload still shows the default, exactly as v4 does, because
 * neither client can read the column back. The gap is a v4-side bug and is
 * reported as such; v5 does not "fix" it by inventing a projection field.
 *
 * **2. No toast system in v5.** v4's `showSuccessToast` / `showErrorToast` copy
 * is carried verbatim into the inline status line under the selects (the
 * established v5 substitute).
 *
 * **3. Every write goes through the `chat` bag.** v4 posts the template /
 * image-profile / announce writes as TOP-LEVEL keys (`{roleplayTemplateId}`)
 * and only the Story's Clock as `{chat: {timelineMode}}`; both shapes land on
 * the same columns via `updateChatSchema`. v5's `chatUpdate` carries only the
 * `chat` bag — the top-level shortcuts are a named server-side deferral
 * (`api/salon.rs:1217`) — so the client uses the bag form throughout, which is
 * the shape the round-2 differential pins.
 *
 * ## Tier-3 deferrals (LOUD — rendered nowhere, nothing stubbed)
 *
 * - **The Concierge tri-state** (v4 :1100): the chat PUT's `conciergeState` key
 *   is a named v5 deferral (`api/salon.rs:1216`).
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
  imports: [Icon],
  template: `
    <div class="qt-chat-sidebar-section qt-chat-sidebar-section-chat flex flex-col gap-3">
      <!-- Agent Mode (v4 :1116-1127). v4 puts this between the Concierge
           tri-state and Roleplay Template; the Concierge control is a named
           deferral, so the badge leads the section. -->
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

      @if (status(); as flash) {
        <p
          class="text-xs"
          [class.qt-text-secondary]="flash.kind === 'success'"
          [class.qt-text-danger]="flash.kind === 'error'"
          role="status"
        >
          {{ flash.message }}
        </p>
      }
    </div>
  `,
})
export class ChatSection {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly state = input.required<ChatSectionState>();
  /** v4 `chatControls.storyBackgroundsEnabled` — gates the regenerate entry. */
  readonly storyBackgroundsEnabled = input(false);
  /** Disables the regenerate entry while a poll is in flight (v5's guard). */
  readonly regeneratingBackground = input(false);
  /** True while this accordion section is open — latches the reference fetches. */
  readonly sectionOpen = input(false);

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
    this.status.set(null);
    try {
      const result = await toggleAgentMode(this.core, this.chatId(), this.localAgentMode());
      this.localAgentMode.set(result.enabled);
      this.status.set({ kind: 'success', message: `Agent mode ${result.status}` });
    } catch (error) {
      this.status.set({
        kind: 'error',
        message: error instanceof Error ? error.message : 'Failed to toggle agent mode',
      });
    } finally {
      this.agentModeSaving.set(false);
    }
  }

  protected async onAvatarGenToggle(): Promise<void> {
    this.avatarGenSaving.set(true);
    try {
      const enabled = await toggleAvatarGeneration(this.core, this.chatId());
      this.status.set({
        kind: 'success',
        message:
          enabled === false ? 'Avatar generation disabled' : 'Avatar generation enabled',
      });
      this.chatUpdated.emit();
    } catch (err) {
      this.status.set({
        kind: 'error',
        message: err instanceof Error ? err.message : 'Failed to toggle avatar generation',
      });
    } finally {
      this.avatarGenSaving.set(false);
    }
  }

  protected readonly templateSaving = signal(false);
  protected readonly imageProfileSaving = signal(false);
  protected readonly alertImagesSaving = signal(false);
  protected readonly timelineModeSaving = signal(false);
  protected readonly status = signal<{ kind: 'success' | 'error'; message: string } | null>(null);

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
  private readonly hasEverOpened = signal(false);

  constructor() {
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
    this.status.set(null);
    try {
      const resp = await this.core.dispatch({
        type: 'chatUpdate',
        chatId: this.chatId(),
        chat: bag,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || fallbackError);
      }
      this.status.set({ kind: 'success', message: successMessage });
      this.chatUpdated.emit();
    } catch (error) {
      revert();
      const msg = error instanceof Error ? error.message : String(error);
      this.status.set({ kind: 'error', message: msg || fallbackError });
    } finally {
      saving.set(false);
    }
  }
}
