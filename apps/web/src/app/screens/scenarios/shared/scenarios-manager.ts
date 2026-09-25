import { ChangeDetectionStrategy, Component, input, signal } from '@angular/core';

import { injectImagesHidden } from '../../../chat/hidden-image/images-hidden';
import type { ScenarioDto } from '../../../core/core-contract';
import { HOST_AVATAR } from '../../../scenario-builder/host-avatar';
import {
  ScenarioBuilderDialog,
  type SaveScenarioTargetKey,
} from '../../../scenario-builder/scenario-builder-dialog';
import type { ScenarioMutator, ScenarioResult } from '../scenarios.api';
import { ScenarioEditorModal, type ScenarioSaveInput } from './scenario-editor-modal';
import { ScenarioRow } from './scenario-row';

/** Which scenarios shelf a manager is showing — General, one project's, or one group's (v4 `08c49319d`). */
export type ScenarioShelf =
  | { kind: 'general' }
  | { kind: 'project'; projectId: string; projectName?: string | null }
  | { kind: 'group'; groupId: string };

/** v4 `shelfSaveTarget` — the shelf's own home, as the save select keys it. */
export function shelfSaveTarget(shelf: ScenarioShelf): SaveScenarioTargetKey {
  switch (shelf.kind) {
    case 'general':
      return 'general';
    case 'project':
      return `project:${shelf.projectId}`;
    case 'group':
      return `group:${shelf.groupId}`;
  }
}

/**
 * ScenariosManager (v4 `components/scenarios/ScenariosManager.tsx`) — the
 * scope-agnostic CRUD body for a `Scenarios/` folder. Both the project
 * Scenarios card and the general `/scenarios` page render this, parameterised
 * only by the {@link ScenarioMutator} the scope-specific factory supplies plus
 * `scopeLabel` / `emptyMessage`. It makes NO dispatches itself — the scope
 * lives entirely in the mutator.
 *
 * Surfaces soft warnings (e.g. multiple default files) above the list; routes
 * save to create-or-update; delete confirms; rename prompts on the FILENAME;
 * set-default re-sends update with `isDefault: true` (no dedicated verb).
 *
 * "Show archived" flips the mutator's FETCH, not a client-side filter: the
 * server decides what is hidden, so the list can never disagree with the API
 * (v4 `d25dacc1`). Archive/restore is single-click — deliberately no confirm,
 * because nothing is destroyed.
 *
 * `shelf` (v4 `08c49319d`) — which shelf this is, for the Host's Scenario
 * Builder button: the builder reads that shelf's stores (a project's, a
 * group's, or just General), launches with no cast and no "Use this scene",
 * and its Save offers every home with this one preselected. After a save
 * (wherever it was filed) this shelf refreshes silently — the shelves are not
 * TanStack-cached, so the `scenarioKeys.all` invalidation the save dialog runs
 * cannot reach them.
 */
@Component({
  selector: 'qt-scenarios-manager',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ScenarioRow, ScenarioEditorModal, ScenarioBuilderDialog],
  template: `
    <div class="space-y-3 @container">
      @if (mutator().warnings().length > 0) {
        <div class="qt-alert-warning space-y-1" role="alert">
          @for (w of mutator().warnings(); track $index) {
            <p class="qt-text-warning">{{ w }}</p>
          }
        </div>
      }

      @if (actionError(); as msg) {
        <div class="qt-alert-error" role="alert">{{ msg }}</div>
      }

      @if (mutator().error(); as msg) {
        <div class="qt-alert-error" role="alert">{{ msg }}</div>
      }

      <div class="flex items-center justify-between gap-3 flex-wrap">
        <label class="flex items-center gap-2 qt-text-small">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="mutator().showArchived()"
            (change)="onShowArchived($event)"
          />
          Show archived
        </label>
        <div class="flex items-center gap-2 flex-wrap">
          @if (shelf()) {
            <button
              type="button"
              class="qt-button qt-button-secondary qt-button-sm inline-flex items-center gap-1.5"
              (click)="builderOpen.set(true)"
            >
              @if (!imagesHidden()) {
                <img [src]="hostAvatar" alt="" class="h-4 w-4 rounded-full" />
              }
              Ask the Host to set the scene
            </button>
          }
          <button
            type="button"
            class="qt-button qt-button-primary qt-button-sm"
            (click)="openCreate()"
          >
            + New scenario
          </button>
        </div>
      </div>

      @if (mutator().loading()) {
        <p class="qt-text-secondary text-sm">Loading scenarios…</p>
      } @else if (mutator().scenarios().length === 0) {
        <p class="qt-text-secondary text-sm">{{ emptyMessage() }}</p>
      } @else {
        <ul class="divide-y qt-border-default">
          @for (scenario of mutator().scenarios(); track scenario.path) {
            <qt-scenario-row
              [scenario]="scenario"
              [scopeLabel]="scopeLabel()"
              (setDefault)="handleSetDefault($event)"
              (edit)="openEdit($event)"
              (rename)="handleRename($event)"
              (delete)="handleDelete($event)"
              (toggleArchived)="handleToggleArchived($event)"
            />
          }
        </ul>
      }
    </div>

    <!--
      The Host's Scenario Builder on a shelf (v4 08c49319d). Deferred — v4
      loads the dialog with next/dynamic so the builder stays out of this
      surface's bundle until the Host is asked; the @defer block is that lazy
      load. No (use): a shelf has no box to use a scene in.
    -->
    @if (builderOpen()) {
      @if (shelf(); as s) {
        @defer {
          <qt-scenario-builder-dialog
            [cast]="[]"
            [projectId]="s.kind === 'project' ? s.projectId : null"
            [projectName]="s.kind === 'project' ? (s.projectName ?? null) : null"
            [groupIds]="s.kind === 'group' ? [s.groupId] : []"
            saveTargets="everywhere"
            [defaultSaveTarget]="saveTargetOf(s)"
            (closed)="builderOpen.set(false)"
            (saved)="onBuiltSceneSaved()"
          />
        }
      }
    }

    @if (editorOpen()) {
      <qt-scenario-editor-modal
        [scenario]="editingScenario()"
        [scopeLabel]="scopeLabel()"
        [onSave]="handleSave"
        (close)="editorOpen.set(false)"
      />
    }
  `,
})
export class ScenariosManager {
  readonly mutator = input.required<ScenarioMutator>();
  /** Used in the default-tag label + the editor's checkbox copy. e.g. "project" / "general". */
  readonly scopeLabel = input<string>('default');
  readonly emptyMessage = input<string>(
    "No scenarios yet. Create one and it'll be offered when starting new chats.",
  );
  /** The shelf being managed; offers the Host's Scenario Builder when set. */
  readonly shelf = input<ScenarioShelf | null>(null);

  protected readonly imagesHidden = injectImagesHidden();
  protected readonly hostAvatar = HOST_AVATAR;
  protected readonly builderOpen = signal(false);
  protected readonly saveTargetOf = shelfSaveTarget;

  /** v4 `onSaved={() => void refresh({ silent: true })}` — this shelf may have gained a row. */
  protected onBuiltSceneSaved(): void {
    void this.mutator().refresh({ silent: true });
  }

  protected readonly editorOpen = signal(false);
  protected readonly editingScenario = signal<ScenarioDto | null>(null);
  protected readonly actionError = signal<string | null>(null);

  protected openCreate(): void {
    this.editingScenario.set(null);
    this.editorOpen.set(true);
  }

  protected openEdit(scenario: ScenarioDto): void {
    this.editingScenario.set(scenario);
    this.editorOpen.set(true);
  }

  /** Stable reference handed to the editor modal (routes create vs update). */
  protected readonly handleSave = async (input: ScenarioSaveInput): Promise<ScenarioResult> => {
    this.actionError.set(null);
    const editing = this.editingScenario();
    if (editing) {
      return this.mutator().updateScenario(editing.path, {
        name: input.name,
        ...(input.description !== undefined && { description: input.description }),
        isDefault: input.isDefault,
        body: input.body,
      });
    }
    if (!input.filename) {
      return { ok: false, error: 'Filename is required for new scenarios.' };
    }
    return this.mutator().createScenario({
      filename: input.filename,
      name: input.name,
      ...(input.description !== undefined && { description: input.description }),
      isDefault: input.isDefault,
      body: input.body,
    });
  };

  protected async handleDelete(scenario: ScenarioDto): Promise<void> {
    if (!window.confirm(`Delete scenario "${scenario.name}"? This cannot be undone.`)) {
      return;
    }
    this.actionError.set(null);
    const result = await this.mutator().deleteScenario(scenario.path);
    if (!result.ok) {
      this.actionError.set(result.error);
    }
  }

  protected async handleRename(scenario: ScenarioDto): Promise<void> {
    // v4 prompts on the FILENAME (not the display name), prefilled, no-op if unchanged/empty.
    const next = window.prompt(`Rename scenario "${scenario.filename}" to:`, scenario.filename);
    if (next === null) {
      return;
    }
    const trimmed = next.trim();
    if (!trimmed || trimmed === scenario.filename) {
      return;
    }
    this.actionError.set(null);
    const result = await this.mutator().renameScenario(scenario.path, trimmed);
    if (!result.ok) {
      this.actionError.set(result.error);
    }
  }

  protected onShowArchived(event: Event): void {
    this.mutator().setShowArchived((event.target as HTMLInputElement).checked);
  }

  /** v4 `handleToggleArchived` — no confirm; archiving hides, it does not destroy. */
  protected async handleToggleArchived(scenario: ScenarioDto): Promise<void> {
    this.actionError.set(null);
    const result = await this.mutator().setScenarioArchived(scenario.path, !scenario.archived);
    if (!result.ok) {
      this.actionError.set(result.error);
    }
  }

  protected async handleSetDefault(scenario: ScenarioDto): Promise<void> {
    if (scenario.isDefault) {
      return;
    }
    this.actionError.set(null);
    const result = await this.mutator().setDefaultScenario(scenario.path);
    if (!result.ok) {
      this.actionError.set(result.error);
    }
  }
}
