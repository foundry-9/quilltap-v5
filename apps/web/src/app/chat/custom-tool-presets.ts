import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../core/core-client';
import type { CustomToolParameterSpec, DocMountFileDto } from '../core/core-contract';
import {
  PRESET_NAME_PATTERN,
  parsePresetFromPath,
  presetSettingsPath,
  sanitizePresetNameInput,
} from '../pascal/tool-presets';
import { ToastService } from '../ui/toast.service';
import { coerceParamValues, initialParamValues, type ParameterFormValues } from './custom-tool-params-form';
import { customToolsKeys } from './custom-tools.api';

/**
 * Presets — parameter values set aside in the character's vault (v4
 * `ToolPresetsSection` in `components/chat/CustomToolRunDialog.tsx`,
 * `c988fbd2`).
 *
 * Save the run form's current values as `Tools/{name}.{preset}.settings.json`
 * in the running character's vault, list what is already there, and deal a
 * chosen one back into the form.
 *
 * Loading is deliberately loose: a preset key lands only where the tool still
 * declares a parameter of that name and the value is a plain primitive;
 * everything else is ignored, and unmentioned parameters keep their current
 * values. A preset saved against an older revision of a tool therefore degrades
 * gracefully instead of erroring.
 *
 * The name input is filtered as typed ({@link sanitizePresetNameInput}) so a
 * character that cannot appear in the filename cannot be entered at all.
 *
 * ## Why a component of its own
 *
 * v5 folds v4's button and dialog into ONE always-mounted popup component, so
 * the popup's own state deliberately outlives a close. The typed preset NAME
 * must not: in v4 it lives inside this section, which unmounts with the form
 * whenever the dialog closes or the operator goes back to the picker. Keeping
 * the split gives that lifecycle back for free — the popup's `@if` destroys
 * this component exactly where React unmounts v4's.
 *
 * ## No new write path
 *
 * Everything here rides the canonical mount-points file verbs
 * (`mountFilesList` / `mountFileRead` / `mountFileWrite`) — the spec doc's
 * doctrinal line, and the reason a preset is an ordinary vault document that
 * shows up in the Scriptorium and rides vault export.
 */
@Component({
  selector: 'qt-custom-tool-presets',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="rounded-lg qt-border p-5 space-y-4">
      <div>
        <h4 class="text-sm font-medium qt-text">Presets</h4>
        <p class="text-xs qt-text-secondary mt-1">
          A hand you set aside earlier — kept in the character’s vault, dealt back on request.
          Saving under an existing name replaces it.
        </p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <select
          class="qt-input text-sm"
          [value]="''"
          [disabled]="busy() || presets().length === 0"
          [attr.aria-label]="loadLabel()"
          (change)="onSelectPreset($any($event.target))"
        >
          <option value="">{{ placeholder() }}</option>
          @for (preset of presets(); track preset) {
            <option [value]="preset">{{ preset }}</option>
          }
        </select>

        <input
          type="text"
          class="qt-input text-sm flex-1 min-w-32 font-mono"
          placeholder="preset-name"
          aria-label="Name to save this preset under"
          [value]="presetName()"
          [disabled]="busy()"
          (input)="onNameInput($any($event.target))"
        />
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="!canSave()"
          (click)="save()"
        >
          {{ savePending() ? 'Saving…' : 'Save preset' }}
        </button>
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="busy()"
          (click)="resetToDefaults()"
        >
          Reset to defaults
        </button>
      </div>

      @if (presetsQuery.isError()) {
        <p class="text-xs qt-text-destructive">{{ listErrorMessage() }}</p>
      }
    </section>
  `,
})
export class CustomToolPresets {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  /** The tool whose presets these are — `name` keys the filenames, `title` the label. */
  readonly toolName = input.required<string>();
  readonly toolTitle = input.required<string>();
  readonly parameters = input.required<Record<string, CustomToolParameterSpec>>();
  /** The running character's vault. The popup only renders this when it is set. */
  readonly vaultMountPointId = input.required<string>();
  /** The form's current values — what a save files, and what a load lands on top of. */
  readonly values = input.required<ParameterFormValues>();
  /** A run is in flight (v4's `disabled`). */
  readonly disabled = input(false);

  /** Replace the whole value set at once — preset loads and resets (v4 `onValuesReplace`). */
  readonly valuesReplace = output<ParameterFormValues>();

  protected readonly presetName = signal('');
  protected readonly loadPending = signal(false);
  protected readonly savePending = signal(false);

  /**
   * Fresh per mount of the form, same doctrine as the roster: the files live in
   * a store the user edits, so a stale list is worse than a round-trip.
   */
  protected readonly presetsQuery = injectQuery(() => ({
    queryKey: customToolsKeys.presets(this.vaultMountPointId(), this.toolName()),
    queryFn: async () => {
      const data = await this.core.dispatchData({
        type: 'mountFilesList',
        mountPointId: this.vaultMountPointId(),
      });
      const files = (data['files'] as DocMountFileDto[]) ?? [];
      return files
        .map((file) => parsePresetFromPath(this.toolName(), file.relativePath))
        .filter((preset): preset is string => preset !== null)
        .sort((a, b) => a.localeCompare(b));
    },
  }));

  protected readonly presets = computed(() => this.presetsQuery.data() ?? []);

  protected readonly busy = computed(
    () => this.disabled() || this.loadPending() || this.savePending(),
  );

  protected readonly canSave = computed(
    () => !this.busy() && PRESET_NAME_PATTERN.test(this.presetName()),
  );

  /** The four states of the dropdown's placeholder option, in v4's own order. */
  protected readonly placeholder = computed(() => {
    if (this.presetsQuery.isLoading()) return 'Consulting the vault…';
    if (this.presets().length === 0) return 'No presets yet';
    if (this.loadPending()) return 'Dealing it back…';
    return 'Load a preset…';
  });

  protected readonly loadLabel = computed(() => `Load a saved preset for ${this.toolTitle()}`);

  protected listErrorMessage(): string {
    return coreErrorMessage(this.presetsQuery.error(), 'The vault would not list its presets.');
  }

  protected onNameInput(target: HTMLInputElement): void {
    const cleaned = sanitizePresetNameInput(target.value);
    // Write the filtered value straight back, so a rejected keystroke never
    // shows up in the box even for a frame (v4's controlled input).
    target.value = cleaned;
    this.presetName.set(cleaned);
  }

  protected onSelectPreset(target: HTMLSelectElement): void {
    const preset = target.value;
    // v4's select is `value=""` always — the choice is an event, not a state.
    target.value = '';
    if (preset) void this.load(preset);
  }

  /**
   * Read one preset and deal it back into the form. The two refusals below are
   * about the FILE, not the tool: a preset is hand-editable, so a person can
   * leave it unparseable or replace the object with a list. Neither is repaired
   * — it stays as written in the vault, and the form is left alone.
   */
  private async load(preset: string): Promise<void> {
    this.loadPending.set(true);
    try {
      const file = await this.core.dispatchData({
        type: 'mountFileRead',
        mountPointId: this.vaultMountPointId(),
        path: presetSettingsPath(this.toolName(), preset),
      });
      let parsed: unknown;
      try {
        parsed = JSON.parse(String(file['content'] ?? ''));
      } catch {
        throw new Error(
          `Preset “${preset}” would not read as JSON. It stays as written in the vault.`,
        );
      }
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
        this.toasts.showError(
          `Preset “${preset}” is not a settings object. It stays as written in the vault.`,
        );
        return;
      }
      const record = parsed as Record<string, unknown>;
      const next: ParameterFormValues = { ...this.values() };
      for (const [name, param] of Object.entries(this.parameters())) {
        if (!(name in record)) continue;
        const v = record[name];
        if (typeof v !== 'string' && typeof v !== 'number' && typeof v !== 'boolean') continue;
        next[name] = param.type === 'boolean' ? Boolean(v) : String(v);
      }
      this.valuesReplace.emit(next);
      // So saving again, values adjusted, updates the hand just taken up.
      this.presetName.set(preset);
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'The preset could not be read.'));
    } finally {
      this.loadPending.set(false);
    }
  }

  /** File the form's COERCED values under the typed name (overwrite semantics). */
  protected async save(): Promise<void> {
    if (!this.canSave()) return;
    const preset = this.presetName();
    this.savePending.set(true);
    try {
      await this.core.dispatchData({
        type: 'mountFileWrite',
        mountPointId: this.vaultMountPointId(),
        path: presetSettingsPath(this.toolName(), preset),
        content: JSON.stringify(coerceParamValues(this.parameters(), this.values()), null, 2),
      });
      this.toasts.showSuccess(`Preset “${preset}” filed in the character's vault.`);
      // Invalidate rather than refetch this one observer: v4 does, and the key
      // is shared by construction — anything else watching this tool's presets
      // in this vault must see the new file too.
      void this.queryClient.invalidateQueries({
        queryKey: customToolsKeys.presets(this.vaultMountPointId(), this.toolName()),
      });
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'The preset could not be saved.'));
    } finally {
      this.savePending.set(false);
    }
  }

  protected resetToDefaults(): void {
    this.valuesReplace.emit(initialParamValues(this.parameters()));
  }
}
