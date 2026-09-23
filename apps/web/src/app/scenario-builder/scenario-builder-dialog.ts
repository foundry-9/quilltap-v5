import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { QuillAnimation } from '../chat/quill-animation';
import { ThinkingBlock } from '../chat/thinking-block';
import { CoreClient } from '../core/core-client';
import { MarkdownField } from '../editor/markdown-field';
import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import { describeHostActivity } from './host-activity';
import { HOST_AVATAR } from './host-avatar';
import {
  SaveScenarioDialog,
  type SavedScenarioTarget,
  type ScenarioBuilderCastMember,
} from './save-scenario-dialog';
import {
  connectionProfilesKey,
  fetchConnectionProfiles,
  fetchScenarioBuilderCapabilities,
  mapScenarioBuilderProfiles,
  scenarioBuilderKeys,
} from './scenario-builder.api';
import { ScenarioBuilderRun } from './scenario-builder-run.state';

export type { SavedScenarioTarget, ScenarioBuilderCastMember } from './save-scenario-dialog';

type Mode = 'real' | 'in-world';

/**
 * The Scenario Builder dialog — "Ask the Host to set the scene." (v4
 * `components/scenario-builder/ScenarioBuilderDialog.tsx` at `d1c06cd9d`.)
 *
 * Three panes switched by state: Inputs (mode, location, time, details, model),
 * Running (the Host's enquiries, live), and Review (an editable draft with a
 * Revise box, Save as scenario…, and Use this scene). Surface-agnostic: the New
 * Chat form and the Salon sidebar each decide what "use" and "saved" mean for
 * their own picker via `use` / `saved`.
 *
 * A failed run never traps a draft: Revise failures leave the draft editable,
 * and Use / Save stay enabled whatever the provider did (v4's header note).
 *
 * The host MOUNTS this only while open (v4 renders it under `builderOpen &&`),
 * so mounting is v4's `isOpen` — the capabilities probe, which v4 enables only
 * while open, simply runs on mount. The run state is provided HERE, so closing
 * the dialog destroys it and aborts any run (v4's unmount abort).
 *
 * Two SPA-convention notes: v4's `closeOnEscape={!running && !saveOpen}` has no
 * counterpart because `qt-modal` handles no Escape at all (the standing modal
 * convention); the ✕ button reaches {@link handleClose}, which refuses while
 * running exactly as v4's does. v4's dialog persists no geometry (measured —
 * `BaseModal` takes none), so neither does this (the order's Tier 2 item 11).
 *
 * @module scenario-builder/scenario-builder-dialog
 */
@Component({
  selector: 'qt-scenario-builder-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MarkdownField, Modal, QuillAnimation, SaveScenarioDialog, ThinkingBlock],
  providers: [ScenarioBuilderRun],
  template: `
    <qt-modal
      title="The Host sets the scene"
      maxWidth="2xl"
      [closeOnBackdrop]="false"
      (close)="handleClose()"
    >
      <div class="flex items-start gap-3 mb-4">
        <img [src]="hostAvatar" alt="The Host" class="w-10 h-10 rounded-full shrink-0" />
        <p class="text-sm qt-text-secondary">
          @switch (view()) {
            @case ('inputs') {
              Tell me where and when, and I shall go and see what the place is like. The scene I
              bring back will leave the company out of it entirely — you may seat whomever you
              please.
            }
            @case ('running') {
              Pray bear with me; I am out making enquiries.
            }
            @case ('review') {
              Here is the scene as I found it. Amend it as you like, or tell me what to change and I
              shall go round again.
            }
          }
        </p>
      </div>

      @if (view() === 'inputs') {
        <div class="space-y-4">
          <fieldset>
            <legend class="qt-label mb-1">Is the place real, or of your own world?</legend>
            <div class="flex gap-4 text-sm">
              <label class="flex items-center gap-2">
                <input
                  type="radio"
                  name="scenario-builder-mode"
                  value="in-world"
                  [checked]="mode() === 'in-world'"
                  (change)="mode.set('in-world')"
                />
                In-world
              </label>
              <label class="flex items-center gap-2">
                <input
                  type="radio"
                  name="scenario-builder-mode"
                  value="real"
                  [checked]="mode() === 'real'"
                  (change)="mode.set('real')"
                />
                Real
              </label>
            </div>
            <p class="mt-1 text-xs qt-text-muted">
              {{
                mode() === 'in-world'
                  ? 'I shall read only the document stores this company could see — never the wider world.'
                  : 'I shall consult the wider world, and your document stores besides.'
              }}
            </p>
          </fieldset>

          <div>
            <label for="scenario-builder-location" class="qt-label mb-1 block">Location</label>
            <input
              id="scenario-builder-location"
              type="text"
              class="qt-input"
              maxlength="500"
              placeholder="the Gare du Nord · the Lantern Inn at Vey's Crossing"
              [value]="location()"
              (input)="location.set($any($event.target).value)"
            />
          </div>

          <div>
            <label for="scenario-builder-time" class="qt-label mb-1 block">Time</label>
            <input
              id="scenario-builder-time"
              type="text"
              class="qt-input"
              maxlength="200"
              placeholder="an autumn evening, 1927 · now · the third day of the siege"
              [value]="time()"
              (input)="time.set($any($event.target).value)"
            />
          </div>

          <div>
            <span class="qt-label mb-1 block">Further details (optional)</span>
            <qt-markdown-field
              ariaLabel="Further details"
              minHeight="4rem"
              [value]="details()"
              (contentChange)="details.set($event)"
            />
            <p class="mt-1 text-xs qt-text-muted">
              Anything else I ought to know — the mood, the weather, what has just happened, how
              long you would like it.
            </p>
          </div>

          <div>
            <label for="scenario-builder-profile" class="qt-label mb-1 block">Model</label>
            <select
              id="scenario-builder-profile"
              class="qt-select"
              [disabled]="profilesLoading() || profiles().length === 0"
              (change)="chosenProfileId.set($any($event.target).value)"
            >
              @for (p of profiles(); track p.id) {
                <option
                  [value]="p.id"
                  [disabled]="!p.allowToolUse"
                  [selected]="p.id === profileId()"
                >
                  {{ p.name }}{{ p.isDefault ? ' (Default)' : ''
                  }}{{ !p.allowToolUse ? ' (no tools)' : '' }}
                </option>
              }
            </select>
            @if (anyNoTools()) {
              <p class="mt-1 text-xs qt-text-muted">
                Profiles with tool use switched off are listed but cannot be chosen: I cannot make
                enquiries without tools.
              </p>
            }
          </div>

          @if (webUnreachable()) {
            <p role="status" class="qt-alert-warning text-sm">
              I regret that the wider world is out of reach on this occasion{{
                selectedProfile() && !selectedProfile()!.allowWebSearch
                  ? ' — this profile does not permit web search'
                  : ' — no search provider has been engaged'
              }}. I shall make do with what I know and your stores, and keep the particulars general
              where I am unsure.
            </p>
          }

          @if (runError(); as message) {
            <p role="alert" class="text-sm qt-text-danger">{{ message }}</p>
          }
        </div>
      }

      @if (view() === 'running') {
        <div class="space-y-4">
          <div class="flex items-center gap-3">
            <qt-quill-animation size="lg" label="The Host is out making enquiries…" />
            <span class="text-sm qt-text-secondary">The Host is out making enquiries…</span>
          </div>
          @if (builder.toolCalls().length > 0) {
            <ul class="space-y-1 text-sm" aria-label="The Host's enquiries">
              @for (call of builder.toolCalls(); track $index) {
                <li class="flex items-start gap-2">
                  @if (call.pending) {
                    <qt-quill-animation size="sm" [label]="null" class="mt-0.5" />
                  } @else if (call.success === false) {
                    <qt-icon
                      name="close"
                      class="w-4 h-4 mt-0.5 qt-text-danger"
                      title="Came to nothing"
                    />
                  } @else {
                    <qt-icon name="check" class="w-4 h-4 mt-0.5 qt-text-success" title="Done" />
                  }
                  <span class="qt-text-secondary break-words">{{ describe(call) }}</span>
                </li>
              }
            </ul>
          }
          <!-- v4's ThinkingBlock renders nothing for blank content and, while
               streaming, forces itself open. -->
          @if (builder.reasoning().trim()) {
            <qt-thinking-block [content]="builder.reasoning()" [collapsed]="false" />
          }
        </div>
      }

      @if (view() === 'review') {
        <div class="space-y-4">
          <qt-markdown-field
            ariaLabel="The scene"
            minHeight="12rem"
            [recordKey]="draftKey()"
            [value]="draft() ?? ''"
            (contentChange)="draft.set($event)"
          />
          <div>
            <label for="scenario-builder-revision" class="qt-label mb-1 block">Revise</label>
            <div class="flex gap-2">
              <input
                id="scenario-builder-revision"
                type="text"
                class="qt-input flex-1"
                maxlength="2000"
                placeholder="make it raining, and two hours later"
                [value]="revision()"
                (input)="revision.set($any($event.target).value)"
                (keydown.enter)="onRevisionEnter($event)"
              />
              <button
                type="button"
                class="qt-button-secondary"
                [disabled]="!revision().trim() || !profileId()"
                (click)="revise()"
              >
                Revise
              </button>
            </div>
          </div>
          @if (runError(); as message) {
            <p role="alert" class="text-sm qt-text-danger">
              {{ message }} Your draft is untouched.
            </p>
          }
        </div>
      }

      <div qt-modal-footer>
        @switch (view()) {
          @case ('inputs') {
            <div class="flex justify-end gap-2">
              <button type="button" class="qt-button-secondary" (click)="handleClose()">
                Cancel
              </button>
              <button
                type="button"
                class="qt-button-primary"
                [disabled]="!canSetScene()"
                (click)="startRun()"
              >
                Set the scene
              </button>
            </div>
          }
          @case ('running') {
            <div class="flex justify-end gap-2">
              <button type="button" class="qt-button-secondary" (click)="builder.stop()">
                <qt-icon name="stop" class="w-4 h-4 mr-1 inline" />
                Stop
              </button>
            </div>
          }
          @case ('review') {
            <div class="flex flex-wrap items-center justify-between gap-2">
              <button
                type="button"
                class="qt-button-secondary"
                [disabled]="!draftHasText()"
                (click)="saveOpen.set(true)"
              >
                Save as scenario…
              </button>
              <div class="flex gap-2">
                <button type="button" class="qt-button-secondary" (click)="handleClose()">
                  Cancel
                </button>
                <button
                  type="button"
                  class="qt-button-primary"
                  [disabled]="!draftHasText()"
                  (click)="handleUse()"
                >
                  Use this scene
                </button>
              </div>
            </div>
          }
        }
      </div>
    </qt-modal>

    @if (saveOpen() && draft() !== null) {
      <qt-save-scenario-dialog
        [body]="draft()!"
        [defaultName]="defaultSaveName() || 'A scene set by the Host'"
        [projectId]="projectId()"
        [projectName]="projectName()"
        [cast]="cast()"
        (closed)="saveOpen.set(false)"
        (saved)="saved.emit($event)"
      />
    }
  `,
})
export class ScenarioBuilderDialog implements OnInit {
  private readonly core = inject(CoreClient);
  protected readonly builder = inject(ScenarioBuilderRun);
  protected readonly hostAvatar = HOST_AVATAR;

  /** The cast — scopes which stores the Host may read, and the save targets. */
  readonly cast = input<readonly ScenarioBuilderCastMember[]>([]);
  readonly projectId = input<string | null>(null);
  readonly projectName = input<string | null>(null);
  /** Set when launched from inside a chat: the Host also sees its current scene. */
  readonly chatId = input<string | null>(null);

  /** v4 `onClose`. */
  readonly closed = output<void>();
  /** "Use this scene": the surface puts the text in its custom box (v4 `onUse`). */
  readonly use = output<string>();
  /** After a save: the surface may select the new preset (v4 `onSaved`). */
  readonly saved = output<SavedScenarioTarget>();

  protected readonly mode = signal<Mode>('real');
  protected readonly location = signal('');
  protected readonly time = signal('');
  protected readonly details = signal('');
  /**
   * Null until the user picks; until then the default profile (else the first
   * usable one) stands in, so it is preselected the moment profiles arrive.
   */
  protected readonly chosenProfileId = signal<string | null>(null);
  protected readonly draft = signal<string | null>(null);
  protected readonly draftKey = signal(0);
  protected readonly revision = signal('');
  protected readonly saveOpen = signal(false);

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: connectionProfilesKey,
    queryFn: () => fetchConnectionProfiles(this.core),
  }));
  protected readonly profiles = computed(() =>
    mapScenarioBuilderProfiles(this.profilesQuery.data() ?? []),
  );
  protected readonly profilesLoading = computed(() => this.profilesQuery.isPending());

  /** v4 `profileId`: the pick, else default-with-tools, else any-with-tools, else the first. */
  protected readonly profileId = computed(() => {
    const chosen = this.chosenProfileId();
    if (chosen) return chosen;
    const profiles = this.profiles();
    const preferred =
      profiles.find((p) => p.isDefault && p.allowToolUse) ??
      profiles.find((p) => p.allowToolUse) ??
      profiles[0];
    return preferred?.id ?? '';
  });

  private readonly capabilitiesQuery = injectQuery(() => ({
    queryKey: scenarioBuilderKeys.capabilities,
    queryFn: () => fetchScenarioBuilderCapabilities(this.core),
  }));

  protected readonly selectedProfile = computed(
    () => this.profiles().find((p) => p.id === this.profileId()) ?? null,
  );
  protected readonly anyNoTools = computed(() => this.profiles().some((p) => !p.allowToolUse));

  /** Real mode, a profile chosen, and either it forbids search or no provider is engaged. */
  protected readonly webUnreachable = computed(() => {
    const profile = this.selectedProfile();
    return (
      this.mode() === 'real' &&
      !!profile &&
      (!profile.allowWebSearch || this.capabilitiesQuery.data()?.webSearchConfigured === false)
    );
  });

  protected readonly running = computed(() => this.builder.phase() === 'running');
  protected readonly view = computed<'inputs' | 'running' | 'review'>(() =>
    this.running() ? 'running' : this.draft() !== null ? 'review' : 'inputs',
  );

  /** A failed run surfaces on whichever pane we land back on. */
  protected readonly runError = computed(() =>
    this.builder.phase() === 'error' ? this.builder.error() : null,
  );

  protected readonly canSetScene = computed(() => {
    const profile = this.selectedProfile();
    return (
      this.location().trim().length > 0 &&
      this.time().trim().length > 0 &&
      !!profile &&
      profile.allowToolUse
    );
  });

  protected readonly draftHasText = computed(() => !!this.draft()?.trim());

  protected readonly defaultSaveName = computed(() =>
    [this.location().trim(), this.time().trim()].filter(Boolean).join(' — ').slice(0, 100),
  );

  ngOnInit(): void {
    // v4's `useState(cast.length > 0 ? 'in-world' : 'real')` — read once, at mount.
    this.mode.set(this.cast().length > 0 ? 'in-world' : 'real');
  }

  protected describe = describeHostActivity;

  /** v4 `startRun`. */
  async startRun(revise?: { priorDraft: string; revision: string }): Promise<void> {
    const profileId = this.profileId();
    if (!profileId) return;
    const scene = await this.builder.run({
      mode: this.mode(),
      location: this.location().trim(),
      time: this.time().trim(),
      details: this.details(),
      connectionProfileId: profileId,
      projectId: this.projectId() ?? null,
      characterIds: this.cast().map((c) => c.id),
      chatId: this.chatId() ?? null,
      ...(revise ?? {}),
    });
    if (scene !== null) {
      this.draft.set(scene);
      this.draftKey.update((k) => k + 1);
      this.revision.set('');
    }
  }

  protected revise(): void {
    const draft = this.draft();
    if (draft === null) return;
    void this.startRun({ priorDraft: draft, revision: this.revision().trim() });
  }

  protected onRevisionEnter(event: Event): void {
    if (this.revision().trim() && this.profileId()) {
      event.preventDefault();
      this.revise();
    }
  }

  /** v4 `handleClose` — refuses while the Host is out. */
  handleClose(): void {
    if (this.running()) return;
    this.closed.emit();
  }

  /** v4 `handleUse`. */
  protected handleUse(): void {
    const draft = this.draft();
    if (draft === null) return;
    this.use.emit(draft);
    this.closed.emit();
  }
}
