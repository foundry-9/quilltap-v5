import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import type { CharacterScenario } from '../../../core/core-contract';
import { MarkdownField } from '../../../editor/markdown-field';
import { Icon } from '../../../ui/icon';
import { PROMPT_FIELD_HINTS } from '../../../ui/prompt-field-hints';
import { newScenario } from './character-form';

/**
 * The inline scenarios array editor (v4 `CharacterBasicInfo.tsx` scenarios
 * block): each row is a title + a content markdown field (v4 `:482`, its
 * `minHeight="6rem"`); "+ Add Scenario" appends a fresh row (client-minted id
 * via `crypto.randomUUID()`); the trash icon removes a row. The whole array
 * round-trips in the parent form's `scenarios` field and is saved as part of
 * the ONE `characterUpdate` bag — v4 instead POSTs each scenario to its own
 * sub-resource immediately; this port defers persistence to the parent's
 * explicit Save (per the work order).
 *
 * `recordKey` is the scenario id (v4 keys its remount
 * `${scenario.id}-${externalUpdateCount}`; the v5 form has no external-update
 * counter, and the id alone covers the row-identity re-key this editor can see).
 *
 * The block keeps its CUSTOM header — v4's `a6870c5a` sweep deliberately did
 * not put `PromptFieldLabel` here, because the "+ Add Scenario" control and the
 * array editor below it are not a labelled field. It gains only what that
 * commit gave it: "— the stage, never the actor" folded into the helper, and
 * the `Written as:` line drawn from `PROMPT_FIELD_HINTS.scenario.example`.
 * Fixed on the way: v5's helper had dropped v4's "Stored in the vault's
 * Scenarios/ folder." clause (a pre-existing transcription gap; v5 does store
 * them there, `db::scenarios::SCENARIOS_FOLDER`).
 */
@Component({
  selector: 'qt-scenario-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MarkdownField],
  template: `
    <div>
      <div class="flex justify-between items-center mb-2">
        <label class="block qt-label">Scenarios (Optional)</label>
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="add()">
          + Add Scenario
        </button>
      </div>
      <p class="text-xs qt-text-secondary mb-1">
        Named settings and contexts for conversations &mdash; the stage, never the actor. Each
        scenario can be selected when starting a chat. Stored in the vault&rsquo;s Scenarios/
        folder.
      </p>
      <p class="text-xs qt-text-secondary mb-3">Written as: <em>{{ scenarioExample }}</em></p>

      @if (scenarios().length === 0) {
        <div class="qt-card text-center py-6">
          <p class="qt-text-small mb-3">
            No scenarios yet. Add one to give this character distinct roleplay contexts.
          </p>
          <button type="button" class="qt-button-primary" (click)="add()">
            Add First Scenario
          </button>
        </div>
      } @else {
        <div class="space-y-3">
          @for (scenario of scenarios(); track scenario.id; let i = $index) {
            <div class="qt-card">
              <div class="flex items-center gap-2 mb-2">
                <input
                  type="text"
                  class="qt-input flex-1"
                  placeholder="Scenario title"
                  [value]="scenario.title"
                  (input)="setTitle(i, $any($event.target).value)"
                />
                <button
                  type="button"
                  class="qt-button-icon qt-button-ghost hover:qt-text-destructive"
                  title="Remove scenario"
                  (click)="remove(i)"
                >
                  <qt-icon name="trash" class="w-4 h-4" />
                </button>
              </div>
              <qt-markdown-field
                ariaLabel="Scenario content"
                minHeight="6rem"
                [recordKey]="scenario.id"
                [value]="scenario.content"
                (contentChange)="setContent(i, $event)"
              />
            </div>
          }
        </div>
      }
    </div>
  `,
})
export class ScenarioEditor {
  readonly scenarios = input.required<CharacterScenario[]>();
  readonly scenariosChange = output<CharacterScenario[]>();

  /** v4 renders the shared hint's example inline here, not the whole label. */
  protected readonly scenarioExample = PROMPT_FIELD_HINTS.scenario.example;

  protected add(): void {
    this.scenariosChange.emit([...this.scenarios(), newScenario()]);
  }

  protected remove(index: number): void {
    this.scenariosChange.emit(this.scenarios().filter((_, i) => i !== index));
  }

  protected setTitle(index: number, title: string): void {
    this.scenariosChange.emit(
      this.scenarios().map((s, i) =>
        i === index ? { ...s, title, updatedAt: new Date().toISOString() } : s,
      ),
    );
  }

  protected setContent(index: number, content: string): void {
    this.scenariosChange.emit(
      this.scenarios().map((s, i) =>
        i === index ? { ...s, content, updatedAt: new Date().toISOString() } : s,
      ),
    );
  }
}
