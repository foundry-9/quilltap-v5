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

import { CoreClient } from '../../core/core-client';
import type { AnnouncerSenderWire, CharacterListItem, StaffSenderWire } from '../../core/core-contract';
import { MarkdownField } from '../../editor/markdown-field';
import { characterKeys, fetchCharacterList } from '../../screens/characters/characters.api';
import { Modal } from '../../ui/modal';
import { QuillAnimation } from '../quill-animation';
import {
  STAFF_OPTIONS,
  announcementKeys,
  fetchAnnouncementProfiles,
  postAnnouncement,
  previewAnnouncement,
} from './post-office.api';

type Mode = 'staff' | 'character' | 'custom';
type Stage = 'compose' | 'generating' | 'review';

/** v4's sentinel for "post it verbatim, do not run it through a model" (`:71`). */
const AS_IS = 'as-is';

/**
 * The Insert Announcement dialog (v4
 * `components/chat/InsertAnnouncementDialog.tsx`): drop an ad-hoc bubble into the
 * scene as one of the Salon's staff, as a character who is NOT in the chat, or
 * under a name you make up on the spot.
 *
 * The character arm is the interesting one. Picking an off-scene character
 * offers a connection profile, and unless the operator chooses "use as-is" the
 * dialog does NOT post what was typed — it asks the server for an in-character
 * rewrite (`chatAnnouncementPreview`, which persists nothing), shows the
 * proposal, and posts only what the operator approves. The proposal is itself
 * editable, "Edit seed" returns to the original, and "Regenerate" asks again.
 * That approve/edit/regenerate loop is the whole point: an LLM writes the line,
 * a human signs it.
 *
 * Two divergences from v4's component, both forced and both v5 precedent:
 *
 *  - v4 hangs this off a draggable `FloatingDialog`; v5 has no such primitive,
 *    so this is a centered `qt-modal` (the `brahma-console-dialog.ts` ruling).
 *  - v4's picker rows render `c.avatarUrl ? <img> : <placeholder>` — but v4's
 *    own `GET /api/v1/characters` never returns an `avatarUrl` (verified at
 *    `e646f58b`: the field appears nowhere in the route or its handlers), so
 *    every row in v4 shows the placeholder circle. The placeholder is carried
 *    here rather than quietly upgraded to v5's real `defaultImage` avatar —
 *    porting vestigial v4 code faithfully and sweeping it deliberately later is
 *    the standing rule, and this one is recorded in the lane record.
 *
 * State resets on each open because the salon renders the dialog conditionally
 * (v4 relies on exactly the same thing, `:98-100`), so every open is a fresh
 * mount.
 */
@Component({
  selector: 'qt-insert-announcement-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, MarkdownField, QuillAnimation],
  template: `
    <qt-modal
      title="Insert Announcement"
      maxWidth="3xl"
      [closeOnBackdrop]="!blocking()"
      (close)="onDialogClose()"
    >
      @if (error(); as msg) {
        <div class="qt-alert-error text-sm mb-4" role="alert">{{ msg }}</div>
      }

      <!-- Mode selector (v4 :347-381) -->
      <div class="mb-4">
        <label class="block text-sm qt-text-primary mb-2">Sender</label>
        <div class="flex gap-2" role="tablist" aria-label="Announcement sender type">
          @for (tab of MODE_TABS; track tab.mode) {
            <button
              type="button"
              role="tab"
              [attr.aria-selected]="mode() === tab.mode"
              [class]="
                'qt-button ' + (mode() === tab.mode ? 'qt-button-primary' : 'qt-button-secondary')
              "
              [disabled]="isPosting() || stage() !== 'compose'"
              (click)="onModeChange(tab.mode)"
            >
              {{ tab.label }}
            </button>
          }
        </div>
      </div>

      <!-- Sender picker (v4 :384-532) -->
      <div class="mb-4">
        @if (mode() === 'staff') {
          <div>
            <label for="announce-staff" class="block text-sm qt-text-primary mb-2">
              Staff member
            </label>
            <select
              id="announce-staff"
              class="qt-input w-full"
              [value]="staffId()"
              [disabled]="isPosting()"
              (change)="staffId.set($any($event.target).value)"
            >
              @for (s of STAFF_OPTIONS; track s.id) {
                <option [value]="s.id" [selected]="s.id === staffId()">{{ s.label }}</option>
              }
            </select>
          </div>
        }

        @if (mode() === 'character') {
          <div>
            <label
              for="announce-character-search"
              class="block text-sm qt-text-primary mb-2"
            >
              Off-scene character
            </label>
            <input
              id="announce-character-search"
              type="text"
              class="qt-input w-full mb-2"
              placeholder="Search by name…"
              [value]="charSearch()"
              [disabled]="isPosting() || charsLoading() || stage() !== 'compose'"
              (input)="charSearch.set($any($event.target).value)"
            />
            @if (charsLoading()) {
              <div class="qt-text-secondary text-sm">Loading characters…</div>
            } @else if (offSceneCharacters().length === 0) {
              <div class="qt-text-secondary text-sm">
                No matching workspace characters are absent from this chat.
              </div>
            } @else {
              <div class="max-h-40 overflow-y-auto qt-border-primary border rounded mb-3">
                @for (c of offSceneCharacters(); track c.id) {
                  <button
                    type="button"
                    [class]="
                      'w-full px-3 py-2 text-left text-sm flex items-center gap-3 ' +
                      (characterId() === c.id ? 'qt-bg-primary/20' : 'hover:qt-bg-primary/10')
                    "
                    [disabled]="isPosting() || stage() !== 'compose'"
                    (click)="onSelectCharacter(c.id)"
                  >
                    <!-- v4's placeholder circle: its characters list carries no
                         avatarUrl, so this is the branch v4 always renders. -->
                    <div class="w-8 h-8 rounded-full qt-bg-secondary flex-shrink-0"></div>
                    <div class="min-w-0">
                      <div class="font-medium truncate">{{ c.name }}</div>
                      @if (c.title) {
                        <div class="text-xs qt-text-secondary truncate">{{ c.title }}</div>
                      }
                    </div>
                  </button>
                }
              </div>
            }

            @if (selectedCharacter(); as sel) {
              <div class="space-y-3">
                <div>
                  <label for="announce-profile" class="block text-sm qt-text-primary mb-2">
                    How should they say it?
                  </label>
                  <select
                    id="announce-profile"
                    class="qt-input w-full"
                    [value]="profileId()"
                    [disabled]="isPosting() || stage() !== 'compose'"
                    (change)="profileOverride.set($any($event.target).value)"
                  >
                    <option [value]="AS_IS" [selected]="profileId() === AS_IS">
                      Use as-is, do not process in-character
                    </option>
                    @for (p of profiles(); track p.id) {
                      <option [value]="p.id" [selected]="p.id === profileId()">
                        {{ p.name }} — {{ p.modelName }}{{ p.isDefault ? ' (default)' : '' }}
                      </option>
                    }
                  </select>
                  @if (sel.controlledBy === 'user' && profileId() === AS_IS) {
                    <div class="qt-text-xs mt-1">
                      {{ sel.name }} is user-controlled — by default the announcement is posted
                      verbatim.
                    </div>
                  }
                </div>

                @if (showSystemPromptPicker()) {
                  <div>
                    <label for="announce-prompt" class="block text-sm qt-text-primary mb-2">
                      System prompt
                    </label>
                    <select
                      id="announce-prompt"
                      class="qt-input w-full"
                      [value]="systemPromptId() || ''"
                      [disabled]="isPosting() || stage() !== 'compose'"
                      (change)="systemPromptOverride.set($any($event.target).value || null)"
                    >
                      @for (p of sel.systemPrompts; track p.id) {
                        <option [value]="p.id" [selected]="p.id === systemPromptId()">
                          {{ p.name }}{{ p.isDefault ? ' (default)' : '' }}
                        </option>
                      }
                    </select>
                  </div>
                }
              </div>
            }
          </div>
        }

        @if (mode() === 'custom') {
          <div>
            <label for="announce-custom-name" class="block text-sm qt-text-primary mb-2">
              Display name
            </label>
            <input
              id="announce-custom-name"
              type="text"
              class="qt-input w-full"
              maxlength="120"
              placeholder="e.g. The Narrator, A Distant Voice"
              [value]="customName()"
              [disabled]="isPosting()"
              (input)="customName.set($any($event.target).value)"
            />
            <div class="qt-text-xs mt-1">
              The bubble shows this name beside a placeholder avatar.
            </div>
          </div>
        }
      </div>

      <!-- Seed editor (v4 :535-558) -->
      <div class="mb-4">
        <div class="flex items-center justify-between mb-2">
          <label class="block text-sm qt-text-primary">
            {{ willRewrite() ? 'What you want the character to announce' : 'Announcement' }}
          </label>
          @if (willRewrite() && stage() === 'review') {
            <button
              type="button"
              class="text-xs qt-text-link underline"
              [disabled]="isPosting()"
              (click)="editSeed()"
            >
              Edit seed
            </button>
          }
        </div>
        <qt-markdown-field
          [value]="content()"
          [disabled]="isPosting() || stage() !== 'compose'"
          minHeight="12rem"
          ariaLabel="Announcement body"
          (contentChange)="content.set($event)"
        />
      </div>

      <!-- Preview panel (v4 :561-581) -->
      @if (willRewrite() && stage() !== 'compose') {
        <div>
          <label class="block text-sm qt-text-primary mb-2">
            What {{ selectedCharacter()?.name || 'the character' }} will say
          </label>
          @if (stage() === 'generating') {
            <div
              class="qt-border-primary border rounded p-6 flex flex-col items-center justify-center gap-3 min-h-32"
            >
              <qt-quill-animation size="lg" />
              <div class="qt-text-secondary text-sm">Generating in character…</div>
            </div>
          } @else {
            <qt-markdown-field
              [value]="proposedMarkdown()"
              [disabled]="isPosting()"
              minHeight="12rem"
              ariaLabel="Proposed announcement"
              (contentChange)="proposedMarkdown.set($event)"
            />
          }
        </div>
      }

      <div qt-modal-footer class="flex items-center justify-end gap-3">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="blocking()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        @if (mode() === 'character' && stage() === 'review') {
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="isPosting() || content().trim().length === 0"
            (click)="runPreview()"
          >
            Regenerate
          </button>
        }
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="!canSubmit()"
          (click)="onPrimary()"
        >
          {{ primaryLabel() }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class InsertAnnouncementDialog {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  /** Character ids already in the chat — filtered OUT of the off-scene picker. */
  readonly participantCharacterIds = input<readonly string[]>([]);

  readonly close = output<void>();
  /** A bubble landed — the salon refetches the chat (v4 `onPosted`). */
  readonly posted = output<void>();

  protected readonly AS_IS = AS_IS;
  protected readonly STAFF_OPTIONS = STAFF_OPTIONS;
  protected readonly MODE_TABS: ReadonlyArray<{ mode: Mode; label: string }> = [
    { mode: 'staff', label: 'Staff' },
    { mode: 'character', label: 'Off-scene character' },
    { mode: 'custom', label: 'Custom' },
  ];

  protected readonly mode = signal<Mode>('staff');
  protected readonly staffId = signal<StaffSenderWire>('host');
  protected readonly characterId = signal('');
  protected readonly customName = signal('');
  protected readonly content = signal('');
  protected readonly charSearch = signal('');
  protected readonly stage = signal<Stage>('compose');
  protected readonly proposedMarkdown = signal('');
  protected readonly isPosting = signal(false);
  /** v4 surfaces failures as toasts; v5 has no toast bus, so an inline alert. */
  protected readonly error = signal<string | null>(null);

  /**
   * Explicit user choices for the in-character rewrite. Null means "use the
   * computed default" — that way the default updates automatically when the
   * character or profile list changes, but a deliberate pick sticks (v4 `:89-93`).
   */
  protected readonly profileOverride = signal<string | null>(null);
  protected readonly systemPromptOverride = signal<string | null>(null);

  /**
   * v4 loads the characters LAZILY — only when the operator switches to the
   * off-scene arm (`loadCharactersIfNeeded`, `:102-115`) — and the connection
   * profiles EAGERLY, once per open (`:119-136`). Both are reproduced by the
   * `enabled` gates rather than by hand-rolled load flags.
   */
  private readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(),
    enabled: this.mode() === 'character',
    queryFn: () => fetchCharacterList(this.core),
  }));
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: announcementKeys.profiles,
    queryFn: () => fetchAnnouncementProfiles(this.core),
  }));

  protected readonly charsLoading = computed(() => this.charactersQuery.isLoading());
  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);
  private readonly characters = computed<CharacterListItem[]>(
    () => this.charactersQuery.data() ?? [],
  );

  protected readonly selectedCharacter = computed<CharacterListItem | null>(
    () => this.characters().find((c) => c.id === this.characterId()) ?? null,
  );

  /**
   * v4 `defaultProfileId` (`:146-156`): user-controlled characters default to
   * as-is; LLM characters fall back through the character's own default →
   * the system default → as-is.
   */
  private readonly defaultProfileId = computed<string>(() => {
    const sel = this.selectedCharacter();
    if (this.mode() !== 'character' || !sel) return AS_IS;
    if (sel.controlledBy === 'user') return AS_IS;
    const profiles = this.profiles();
    if (
      sel.defaultConnectionProfileId &&
      profiles.some((p) => p.id === sel.defaultConnectionProfileId)
    ) {
      return sel.defaultConnectionProfileId;
    }
    return profiles.find((p) => p.isDefault)?.id ?? AS_IS;
  });

  /** v4 `defaultSystemPromptId` (`:160-170`): explicit → isDefault → first. */
  private readonly defaultSystemPromptId = computed<string | null>(() => {
    const sel = this.selectedCharacter();
    if (this.mode() !== 'character' || !sel) return null;
    const prompts = sel.systemPrompts ?? [];
    if (sel.defaultSystemPromptId && prompts.some((p) => p.id === sel.defaultSystemPromptId)) {
      return sel.defaultSystemPromptId;
    }
    return prompts.find((p) => p.isDefault)?.id ?? prompts[0]?.id ?? null;
  });

  protected readonly profileId = computed(() => this.profileOverride() ?? this.defaultProfileId());
  protected readonly systemPromptId = computed(
    () => this.systemPromptOverride() ?? this.defaultSystemPromptId(),
  );

  /** v4 `offSceneCharacters` (`:196-206`) — absent from the chat, name-filtered, sorted. */
  protected readonly offSceneCharacters = computed<CharacterListItem[]>(() => {
    const present = new Set(this.participantCharacterIds());
    const search = this.charSearch().trim().toLowerCase();
    return this.characters()
      .filter((c) => !present.has(c.id))
      .filter((c) => (search.length === 0 ? true : c.name.toLowerCase().includes(search)))
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  protected readonly willRewrite = computed(
    () => this.mode() === 'character' && this.profileId() !== AS_IS,
  );

  /** v4 gates the prompt picker on MORE THAN ONE prompt (`:325-329`). */
  protected readonly showSystemPromptPicker = computed(() => {
    const sel = this.selectedCharacter();
    return (
      this.mode() === 'character' &&
      !!sel &&
      (sel.systemPrompts?.length ?? 0) > 1 &&
      this.profileId() !== AS_IS
    );
  });

  /** v4 `canSubmit` (`:210-220`). */
  protected readonly canSubmit = computed<boolean>(() => {
    if (this.isPosting() || this.stage() === 'generating') return false;
    if (this.mode() === 'staff') return !!this.staffId() && this.content().trim().length > 0;
    if (this.mode() === 'custom') {
      return this.customName().trim().length > 0 && this.content().trim().length > 0;
    }
    if (this.mode() === 'character') {
      if (!this.characterId()) return false;
      if (this.stage() === 'compose') return this.content().trim().length > 0;
      if (this.stage() === 'review') return this.proposedMarkdown().trim().length > 0;
    }
    return false;
  });

  /** v4 `primaryLabel` (`:317-323`). */
  protected readonly primaryLabel = computed(() => {
    if (this.stage() === 'generating') return 'Generating…';
    if (this.isPosting()) return 'Posting…';
    if (this.mode() === 'character' && this.stage() === 'review') return 'Post Announcement';
    if (this.willRewrite() && this.stage() === 'compose') return 'Preview in character';
    return 'Post Announcement';
  });

  /** v4 `dialogClose` (`:331`) — the ✕/backdrop are inert while work is in flight. */
  protected readonly blocking = computed(() => this.isPosting() || this.stage() === 'generating');

  protected onDialogClose(): void {
    if (this.blocking()) return;
    this.close.emit();
  }

  /** v4 `handleSelectCharacter` (`:175-183`) — fresh defaults, clean compose stage. */
  protected onSelectCharacter(id: string): void {
    this.characterId.set(id);
    this.profileOverride.set(null);
    this.systemPromptOverride.set(null);
    this.stage.set('compose');
    this.proposedMarkdown.set('');
  }

  /** v4 `handleModeChange` (`:185-194`). */
  protected onModeChange(next: Mode): void {
    this.mode.set(next);
    this.stage.set('compose');
    this.proposedMarkdown.set('');
    this.profileOverride.set(null);
    this.systemPromptOverride.set(null);
    // (v4's `loadCharactersIfNeeded()` is the query's own `enabled` gate here.)
  }

  /** v4 `editSeed` (`:312-315`). */
  protected editSeed(): void {
    this.stage.set('compose');
    this.proposedMarkdown.set('');
  }

  /** v4 `handlePrimary` (`:299-310`) — review posts the proposal, compose previews or posts. */
  protected onPrimary(): void {
    if (!this.canSubmit()) return;
    if (this.mode() === 'character' && this.stage() === 'review') {
      void this.post(this.proposedMarkdown());
      return;
    }
    if (this.willRewrite() && this.stage() === 'compose') {
      void this.runPreview();
      return;
    }
    void this.post(this.content());
  }

  /** v4 `runPreview` (`:259-297`). */
  protected async runPreview(): Promise<void> {
    if (!this.characterId() || this.profileId() === AS_IS) return;
    this.stage.set('generating');
    this.proposedMarkdown.set('');
    this.error.set(null);
    try {
      const proposed = await previewAnnouncement(this.core, {
        chatId: this.chatId(),
        seedMarkdown: this.content().trim(),
        characterId: this.characterId(),
        connectionProfileId: this.profileId(),
        systemPromptId: this.systemPromptId(),
      });
      if (!proposed) {
        this.error.set('The LLM returned no content. Try again or use as-is.');
        this.stage.set('compose');
        return;
      }
      this.proposedMarkdown.set(proposed);
      this.stage.set('review');
    } catch (err) {
      this.error.set(this.message(err, 'Failed to generate in-character announcement'));
      this.stage.set('compose');
    }
  }

  /** v4 `postAnnouncement` (`:222-257`). */
  private async post(textToPost: string): Promise<void> {
    const sender: AnnouncerSenderWire =
      this.mode() === 'staff'
        ? { kind: 'staff', staffId: this.staffId() }
        : this.mode() === 'character'
          ? { kind: 'character', characterId: this.characterId() }
          : { kind: 'custom', displayName: this.customName().trim() };

    this.isPosting.set(true);
    this.error.set(null);
    try {
      await postAnnouncement(this.core, {
        chatId: this.chatId(),
        contentMarkdown: textToPost.trim(),
        sender,
      });
      this.posted.emit();
      this.close.emit();
    } catch (err) {
      this.error.set(this.message(err, 'Failed to post announcement'));
    } finally {
      this.isPosting.set(false);
    }
  }

  private message(err: unknown, fallback: string): string {
    return (err instanceof Error && err.message) || fallback;
  }
}
