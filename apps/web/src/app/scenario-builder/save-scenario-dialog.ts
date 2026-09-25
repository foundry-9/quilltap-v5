import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  OnInit,
  afterNextRender,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { CoreRequest, ScenarioCreateBag } from '../core/core-contract';
import { scenarioKeys } from '../scenario/scenario.api';
import { characterKeys, fetchCharacterList } from '../screens/characters/characters.api';
import { fetchGroups, groupKeys } from '../screens/groups/groups.api';
import { fetchProjects, projectKeys } from '../screens/prospero/projects.api';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { fetchGroupsByCharacters, groupsByCharactersKey } from './scenario-builder.api';

/** One cast member — whose vault and groups scope the Host, and the save targets. */
export interface ScenarioBuilderCastMember {
  id: string;
  name: string;
}

/** Where a scene was filed — enough for a surface to select it in its picker. */
export type SavedScenarioTarget =
  | { kind: 'general'; path: string }
  | { kind: 'project'; projectId: string; path: string }
  | { kind: 'group'; groupId: string; path: string }
  | { kind: 'character'; characterId: string; scenarioId: string; title: string; content: string };

/**
 * A save target as the Location select spells it (v4 `08c49319d`
 * `SaveScenarioTargetKey`): `general`, `project:<id>`, `group:<id>` or
 * `character:<id>`.
 */
export type SaveScenarioTargetKey =
  'general' | `project:${string}` | `group:${string}` | `character:${string}`;

interface NamedRow {
  id: string;
  name: string;
}

/** v4's `byName` — `a.name.localeCompare(b.name)`, the SPA's own list idiom. */
const byName = (a: NamedRow, b: NamedRow): number => a.name.localeCompare(b.name);

/** v4's refusal when nothing is named (`SaveScenarioDialog.tsx`). */
export const SAVE_NEEDS_A_NAME = 'A scenario wants a name before it can be filed.';

/**
 * "Save as scenario…" — files the Host's scene in one of the four scenario
 * homes (v4 `components/scenario-builder/SaveScenarioDialog.tsx` at
 * `08c49319d`): Quilltap General, a project, a group, or one character's own
 * scenarios. Each goes through that tier's EXISTING create verb — no new
 * storage — and a collision comes back as a refusal that keeps the dialog open
 * so the user can rename.
 *
 * Which homes are offered depends on where the builder was opened. Beside a
 * chat's scenario box (`targets: 'cast'`, the default) they are the ones that
 * chat could use: General, its project, its cast's groups, its cast. From a
 * scenarios shelf (`targets: 'everywhere'`) every home is offered — every
 * project, every group, every live character — with the shelf's own home
 * (`defaultTarget`) preselected once the list that offers it has arrived, and
 * General until then (or when it is never on offer). A pick the user made
 * stands. The select's keys are `project:<id>` in EVERY mode, it groups its
 * options under `Projects` / `Groups` / `Characters` (each only when
 * non-empty, cast mode too), and groups are name-sorted in both modes while a
 * cast keeps its own order — all three are `08c49319d` changes its commit
 * message does not mention.
 *
 * The everywhere lists come through the EXISTING fetchers that own those query
 * keys (`groupKeys.list()` / `projectKeys.list()` / `characterKeys.list()`),
 * never v4's `{ groups }` envelopes stored under a shared key. The character
 * list leaves archived characters out already (a tombstone takes no new
 * scenarios).
 *
 * The group list is `groupList { characterIds }` (the round's §S.2 — v5 has no
 * REST groups edge), keyed as v4 keys it and asked only for a non-empty cast
 * (v4's `enabled: isOpen && castKey.length > 0`; the dialog is mounted only
 * while open, so mounting IS `isOpen`).
 *
 * v4's two failure strings carry an HTTP status (`The scenario could not be
 * filed (HTTP ${res.status}).`); a v5 refusal envelope has no status, so a
 * refusal shows its `message` as-is (v4's `data.error ||` first arm) and an
 * empty one falls to v4's catch sentence, `The scenario could not be filed.`
 *
 * @module scenario-builder/save-scenario-dialog
 */
@Component({
  selector: 'qt-save-scenario-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal
      title="File this scene as a scenario"
      maxWidth="md"
      [closeOnBackdrop]="false"
      (close)="onModalClose()"
    >
      <div class="space-y-4">
        <div>
          <label for="save-scenario-name" class="qt-label mb-1 block">Name</label>
          <input
            id="save-scenario-name"
            type="text"
            class="qt-input"
            maxlength="100"
            [value]="name()"
            [disabled]="saving()"
            (input)="name.set($any($event.target).value)"
          />
        </div>
        <div>
          <label for="save-scenario-description" class="qt-label mb-1 block"
            >Description (optional)</label
          >
          <input
            id="save-scenario-description"
            type="text"
            class="qt-input"
            maxlength="500"
            [value]="description()"
            [disabled]="saving() || isCharacterTarget()"
            (input)="description.set($any($event.target).value)"
          />
          @if (isCharacterTarget()) {
            <p class="mt-1 text-xs qt-text-muted">
              A character&rsquo;s own scenarios keep a title and a body only.
            </p>
          }
        </div>
        <div>
          <label for="save-scenario-target" class="qt-label mb-1 block">Where it lives</label>
          <select
            id="save-scenario-target"
            class="qt-select"
            [disabled]="saving()"
            (change)="chosenTarget.set($any($event.target).value)"
          >
            <option value="general" [selected]="target() === 'general'">Quilltap General</option>
            @if (projects().length > 0) {
              <optgroup label="Projects">
                @for (p of projects(); track p.id) {
                  <option [value]="'project:' + p.id" [selected]="target() === 'project:' + p.id">
                    Project: {{ p.name }}
                  </option>
                }
              </optgroup>
            }
            @if (groups().length > 0) {
              <optgroup label="Groups">
                @for (g of groups(); track g.id) {
                  <option [value]="'group:' + g.id" [selected]="target() === 'group:' + g.id">
                    Group: {{ g.name }}
                  </option>
                }
              </optgroup>
            }
            @if (characters().length > 0) {
              <optgroup label="Characters">
                @for (c of characters(); track c.id) {
                  <option
                    [value]="'character:' + c.id"
                    [selected]="target() === 'character:' + c.id"
                  >
                    {{ c.name }}&rsquo;s scenarios
                  </option>
                }
              </optgroup>
            }
          </select>
        </div>
        @if (error(); as message) {
          <p role="alert" class="text-sm qt-text-danger">{{ message }}</p>
        }
      </div>
      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          class="qt-button-secondary"
          [disabled]="saving()"
          (click)="closed.emit()"
        >
          Cancel
        </button>
        <button type="button" class="qt-button-primary" [disabled]="saving()" (click)="save()">
          {{ saving() ? 'Filing…' : 'Save' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class SaveScenarioDialog implements OnInit {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly toasts = inject(ToastService);

  constructor() {
    // The portal — v4's `BaseModal` renders every modal through
    // `createPortal(…, document.body)` (`components/ui/BaseModal.tsx:96-125`
    // at `d1c06cd9d`, "avoiding stacking context issues"), and v4's save
    // dialog is its OWN `BaseModal` (`SaveScenarioDialog.tsx:13,158`), so it
    // lands on the body beside the builder's rather than nested inside it.
    // The shared `qt-modal` renders in place, so the host moves itself (the
    // `image-detail-modal.ts` pattern). After the first render, never in the
    // constructor: the builder mounts this under `@if`, and an
    // embedded view's insertion would put a constructor-moved host straight
    // back (`angular-body-portal-must-wait-for-render`).
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    inject(DestroyRef).onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') {
        document.body.appendChild(host);
      }
    });
  }

  /** The scene body to file. */
  readonly body = input.required<string>();
  readonly defaultName = input('');
  readonly projectId = input<string | null>(null);
  readonly projectName = input<string | null>(null);
  /** The cast — their groups and their own scenario lists are offered. */
  readonly cast = input<readonly ScenarioBuilderCastMember[]>([]);
  /** `cast` (default): the homes this chat could use. `everywhere`: every home there is. */
  readonly targets = input<'cast' | 'everywhere'>('cast');
  /** Preselected home; falls back to General when it is not on offer. */
  readonly defaultTarget = input<SaveScenarioTargetKey | undefined>(undefined);

  readonly closed = output<void>();
  readonly saved = output<SavedScenarioTarget>();

  protected readonly name = signal('');
  protected readonly description = signal('');
  /** Null until the user picks; until then the preselected home stands in. */
  protected readonly chosenTarget = signal<string | null>(null);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  /** v4's `castKey`: the SORTED, comma-joined cast ids. */
  private readonly castKey = computed(() =>
    this.cast()
      .map((c) => c.id)
      .sort()
      .join(','),
  );

  private readonly everywhere = computed(() => this.targets() === 'everywhere');

  private readonly castGroupsQuery = injectQuery(() => ({
    queryKey: groupsByCharactersKey(this.castKey()),
    queryFn: () => fetchGroupsByCharacters(this.core, this.castKey().split(',')),
    enabled: !this.everywhere() && this.castKey().length > 0,
  }));
  private readonly allGroupsQuery = injectQuery(() => ({
    queryKey: groupKeys.list(),
    queryFn: () => fetchGroups(this.core),
    enabled: this.everywhere(),
  }));
  private readonly projectsQuery = injectQuery(() => ({
    queryKey: projectKeys.list(),
    queryFn: () => fetchProjects(this.core),
    enabled: this.everywhere(),
  }));
  private readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(),
    queryFn: () => fetchCharacterList(this.core),
    enabled: this.everywhere(),
  }));

  protected readonly projects = computed<NamedRow[]>(() => {
    if (this.everywhere()) {
      return (this.projectsQuery.data() ?? []).map(named).sort(byName);
    }
    const projectId = this.projectId();
    return projectId ? [{ id: projectId, name: this.projectName() || 'this project' }] : [];
  });
  protected readonly groups = computed<NamedRow[]>(() =>
    (this.everywhere() ? (this.allGroupsQuery.data() ?? []) : (this.castGroupsQuery.data() ?? []))
      .map(named)
      .sort(byName),
  );
  protected readonly characters = computed<NamedRow[]>(() =>
    this.everywhere()
      ? (this.charactersQuery.data() ?? []).map(named).sort(byName)
      : this.cast().map(named),
  );

  private readonly offered = computed(
    () =>
      new Set<string>([
        'general',
        ...this.projects().map((p) => `project:${p.id}`),
        ...this.groups().map((g) => `group:${g.id}`),
        ...this.characters().map((c) => `character:${c.id}`),
      ]),
  );

  /** v4 `chosenTarget ?? (defaultTarget && offered.has(defaultTarget) ? defaultTarget : 'general')`. */
  protected readonly target = computed(() => {
    const chosen = this.chosenTarget();
    if (chosen !== null) return chosen;
    const preferred = this.defaultTarget();
    return preferred && this.offered().has(preferred) ? preferred : 'general';
  });
  protected readonly isCharacterTarget = computed(() => this.target().startsWith('character:'));

  ngOnInit(): void {
    // v4 seeds `useState(defaultName)` once, at mount.
    this.name.set(this.defaultName());
  }

  protected onModalClose(): void {
    // v4 `onClose={saving ? () => {} : onClose}`.
    if (this.saving()) return;
    this.closed.emit();
  }

  /** v4 `handleSave`. */
  async save(): Promise<void> {
    const trimmedName = this.name().trim();
    if (!trimmedName) {
      this.error.set(SAVE_NEEDS_A_NAME);
      return;
    }
    this.saving.set(true);
    this.error.set(null);

    const description = this.description().trim();
    const fileBody: ScenarioCreateBag = {
      filename: trimmedName,
      name: trimmedName,
      ...(description && { description }),
      body: this.body(),
    };
    const target = this.target();

    try {
      let request: CoreRequest;
      if (target === 'general') {
        request = { type: 'scenarioCreate', scenario: fileBody };
      } else if (target.startsWith('project:')) {
        request = {
          type: 'projectScenarioCreate',
          projectId: target.slice('project:'.length),
          scenario: fileBody,
        };
      } else if (target.startsWith('group:')) {
        request = {
          type: 'groupScenarioCreate',
          groupId: target.slice('group:'.length),
          scenario: fileBody,
        };
      } else if (target.startsWith('character:')) {
        request = {
          type: 'characterScenarioCreate',
          characterId: target.slice('character:'.length),
          title: trimmedName,
          content: this.body(),
        };
      } else {
        this.error.set('Choose where the scenario should live.');
        return;
      }

      const resp = await this.core.dispatch(request);
      if (resp.type === 'error') {
        this.error.set(resp.data.message || 'The scenario could not be filed.');
        return;
      }
      const data = (resp.data ?? {}) as { path?: string; scenario?: { id?: string } };

      let saved: SavedScenarioTarget | null = null;
      if (target === 'general' && data.path) {
        saved = { kind: 'general', path: data.path };
      } else if (target.startsWith('project:') && data.path) {
        saved = { kind: 'project', projectId: target.slice('project:'.length), path: data.path };
      } else if (target.startsWith('group:') && data.path) {
        saved = { kind: 'group', groupId: target.slice('group:'.length), path: data.path };
      } else if (target.startsWith('character:') && data.scenario?.id) {
        // For a vault-backed character this is the PROJECTED id (bug 165,
        // the round's §S.3), so the picker can select it at once.
        saved = {
          kind: 'character',
          characterId: target.slice('character:'.length),
          scenarioId: data.scenario.id,
          title: trimmedName,
          content: this.body(),
        };
      }

      await this.queryClient.invalidateQueries({ queryKey: scenarioKeys.all });
      this.toasts.showSuccess(`“${trimmedName}” has been filed among the scenarios.`);
      if (saved) this.saved.emit(saved);
      this.closed.emit();
    } catch (err) {
      this.error.set((err instanceof Error && err.message) || 'The scenario could not be filed.');
    } finally {
      this.saving.set(false);
    }
  }
}

/** A list row as the select needs it — `id` + `name`, whatever else it carries. */
function named(row: { id: string; name: string }): NamedRow {
  return { id: row.id, name: row.name };
}
