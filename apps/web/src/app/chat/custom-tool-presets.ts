import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../core/core-client';
import type {
  CustomToolParameterSpec,
  CustomToolPreset,
  CustomToolPresetStore,
} from '../core/core-contract';
import {
  PRESET_NAME_PATTERN,
  presetSettingsPath,
  sanitizePresetNameInput,
} from '../pascal/tool-presets';
import { ToastService } from '../ui/toast.service';
import { coerceParamValues, initialParamValues, type ParameterFormValues } from './custom-tool-params-form';
import { customToolsKeys, fetchCustomToolPresets } from './custom-tools.api';

/**
 * Presets — parameter values set aside in the stores the tool is found in (v4
 * `ToolPresetsSection` in `components/chat/CustomToolRunDialog.tsx`,
 * `fd165fce0`, at the pin `624d0d25c`).
 *
 * Save the form's current values as `Tools/{name}.{preset}.settings.json`,
 * list what is already set aside, and deal a chosen one back into the form.
 *
 * Presets are found where the tool itself is found: the character's vault,
 * their groups' stores, the project's, Quilltap General — nearest first, a
 * nearer preset of a name shadowing a farther one. The server walks it
 * (`chatCustomToolPresets`, v4's `?action=presets`); this only shows the
 * result. Saving files into whichever store on that walk is chosen, the
 * character's own vault by default, so a personal preset shadows a shared one
 * of the same name rather than overwriting it.
 *
 * Loading is deliberately loose: a preset key lands only where the tool still
 * declares a parameter of that name and the value is a plain primitive;
 * everything else is ignored, and unmentioned parameters keep their current
 * values. A preset saved against an older revision of a tool therefore
 * degrades gracefully instead of erroring.
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
 * The LIST rides the presets verb (the server's tool walk). Reads and writes
 * still ride the canonical mount-points file verbs (`mountFileRead` /
 * `mountFileWrite`), aimed at the preset's OWN store — the reason a preset is
 * an ordinary document that shows up in the Scriptorium and rides export.
 *
 * ## Inert without a walk (P4.D388 PR-6)
 *
 * `chatId` and `asCharacterId` are optional so the section compiles under a
 * popup that does not bind them yet (P4.D388 HANDOFF H1 to P4.D385): with
 * either one null it renders nothing and dispatches nothing.
 */
@Component({
  selector: 'qt-custom-tool-presets',
  changeDetection: ChangeDetectionStrategy.OnPush,
  // An unknown element defaults to `display: inline`, and vertical margins do
  // not apply to inline boxes — so without this the popup's `space-y-5` gap
  // vanishes and the card butts flush against "Roll privately". The same bug
  // class as `qt-collapsible-card` (P4.9E1B, `_surfaces.css`), which went
  // beyond cosmetics and swallowed clicks; caught here by the unification
  // review before it shipped.
  styles: [':host { display: block; }'],
  template: `
    @if (active()) {
      <section class="rounded-lg qt-border p-5 space-y-4">
        <div>
          <h4 class="text-sm font-medium qt-text">Presets</h4>
          <p class="text-xs qt-text-secondary mt-1">
            A hand you set aside earlier, dealt back on request. They are kept wherever the tool
            itself might be: the character&rsquo;s vault, their groups&rsquo; stores, the
            project&rsquo;s, or Quilltap General &mdash; and the nearest one of a name wins. Saving
            under an existing name in the same store replaces it.
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
            @for (preset of presets(); track preset.name) {
              <option [value]="preset.name" [attr.title]="shadowTitle(preset)">
                {{ preset.name }} — {{ preset.mountName }}
              </option>
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

        @if (stores().length > 0) {
          <div class="flex flex-wrap items-center gap-2">
            <label [attr.for]="storeSelectId()" class="text-xs qt-text-secondary">Save into</label>
            <select
              [id]="storeSelectId()"
              class="qt-input text-sm"
              [disabled]="busy()"
              (change)="onPickStore($any($event.target))"
            >
              @for (store of stores(); track store.mountPointId) {
                <option
                  [value]="store.mountPointId"
                  [selected]="store.mountPointId === saveStore()?.mountPointId"
                >
                  {{ store.mountName }}
                </option>
              }
            </select>
          </div>
        }

        @if (presetsQuery.isError()) {
          <p class="text-xs qt-text-destructive">{{ listErrorMessage() }}</p>
        }
        @if (unread().length > 0) {
          <p class="text-xs qt-text-secondary">
            Could not look in {{ unreadNames() }} &mdash; any presets kept there are missing from
            this list.
          </p>
        }
      </section>
    }
  `,
})
export class CustomToolPresets {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  /** The chat whose roster the tool came from (null until the popup binds it — PR-6). */
  readonly chatId = input<string | null>(null);
  /** The character the tool runs as — whose walk the presets are read along. */
  readonly asCharacterId = input<string | null>(null);
  /** v4 `toolKey(tool)` — `${name}::${asCharacterId ?? ''}`, for the store select's id. */
  readonly toolKey = input<string>('');
  /** The tool whose presets these are — `name` keys the filenames, `title` the label. */
  readonly toolName = input.required<string>();
  readonly toolTitle = input.required<string>();
  readonly parameters = input.required<Record<string, CustomToolParameterSpec>>();
  /** The running character's vault: now only the DEFAULT save store. */
  readonly vaultMountPointId = input<string | null | undefined>(null);
  /** The form's current values — what a save files, and what a load lands on top of. */
  readonly values = input.required<ParameterFormValues>();
  /** A run is in flight (v4's `disabled`). */
  readonly disabled = input(false);

  /** Replace the whole value set at once — preset loads and resets (v4 `onValuesReplace`). */
  readonly valuesReplace = output<ParameterFormValues>();

  protected readonly presetName = signal('');
  /** The chosen save destination; null until the operator picks one. */
  protected readonly pickedStoreId = signal<string | null>(null);
  protected readonly loadPending = signal(false);
  protected readonly savePending = signal(false);

  protected readonly active = computed(() => !!this.chatId() && !!this.asCharacterId());

  private readonly presetsKey = computed(() =>
    customToolsKeys.presets(this.chatId() ?? '', this.asCharacterId(), this.toolName()),
  );

  /**
   * Fresh per mount of the form, same doctrine as the roster: the files live in
   * stores the user edits, so a stale list is worse than a round-trip.
   */
  protected readonly presetsQuery = injectQuery(() => ({
    queryKey: this.presetsKey(),
    enabled: this.active(),
    queryFn: () =>
      fetchCustomToolPresets(
        this.core,
        this.chatId() as string,
        this.toolName(),
        this.asCharacterId() as string,
      ),
  }));

  protected readonly presets = computed(() => this.presetsQuery.data()?.presets ?? []);
  protected readonly stores = computed(() => this.presetsQuery.data()?.stores ?? []);
  protected readonly unread = computed(() => this.presetsQuery.data()?.unread ?? []);
  protected readonly unreadNames = computed(() =>
    this.unread()
      .map((store) => store.mountName)
      .join(', '),
  );

  /**
   * The character's own vault unless the operator chose otherwise; the nearest
   * store on the walk when there is no vault to default to.
   */
  protected readonly saveStore = computed<CustomToolPresetStore | null>(() => {
    const stores = this.stores();
    return (
      stores.find((store) => store.mountPointId === this.pickedStoreId()) ??
      stores.find((store) => store.mountPointId === this.vaultMountPointId()) ??
      stores[0] ??
      null
    );
  });

  protected readonly busy = computed(
    () => this.disabled() || this.loadPending() || this.savePending(),
  );

  protected readonly canSave = computed(
    () => !this.busy() && this.saveStore() !== null && PRESET_NAME_PATTERN.test(this.presetName()),
  );

  protected readonly storeSelectId = computed(() => `custom-tool-${this.toolKey()}-preset-store`);

  /** The four states of the dropdown's placeholder option, in v4's own order. */
  protected readonly placeholder = computed(() => {
    if (this.presetsQuery.isLoading()) return 'Consulting the stores…';
    if (this.presets().length === 0) return 'No presets yet';
    if (this.loadPending()) return 'Dealing it back…';
    return 'Load a preset…';
  });

  protected readonly loadLabel = computed(() => `Load a saved preset for ${this.toolTitle()}`);

  protected shadowTitle(preset: CustomToolPreset): string | null {
    return preset.shadows.length > 0
      ? `Also in ${preset.shadows.map((store) => store.mountName).join(', ')}, which this one outranks`
      : null;
  }

  protected listErrorMessage(): string {
    return coreErrorMessage(this.presetsQuery.error(), 'The stores would not list their presets.');
  }

  protected onNameInput(target: HTMLInputElement): void {
    const cleaned = sanitizePresetNameInput(target.value);
    // Write the filtered value straight back, so a rejected keystroke never
    // shows up in the box even for a frame (v4's controlled input).
    target.value = cleaned;
    this.presetName.set(cleaned);
  }

  protected onPickStore(target: HTMLSelectElement): void {
    this.pickedStoreId.set(target.value);
  }

  protected onSelectPreset(target: HTMLSelectElement): void {
    const name = target.value;
    // v4's select is `value=""` always — the choice is an event, not a state.
    target.value = '';
    const chosen = this.presets().find((preset) => preset.name === name);
    if (chosen) void this.load(chosen);
  }

  /**
   * Read one preset from its own store and deal it back into the form. The two
   * refusals below are about the FILE, not the tool: a preset is hand-editable,
   * so a person can leave it unparseable or replace the object with a list.
   * Neither is repaired — it stays as written in its store, and the form is
   * left alone.
   */
  private async load(preset: CustomToolPreset): Promise<void> {
    this.loadPending.set(true);
    try {
      const file = await this.core.dispatchData({
        type: 'mountFileRead',
        mountPointId: preset.mountPointId,
        path: preset.path,
      });
      let parsed: unknown;
      try {
        parsed = JSON.parse(String(file['content'] ?? ''));
      } catch {
        throw new Error(
          `Preset “${preset.name}” would not read as JSON. It stays as written in ${preset.mountName}.`,
        );
      }
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
        this.toasts.showError(
          `Preset “${preset.name}” is not a settings object. It stays as written in ${preset.mountName}.`,
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
      // So saving again, values adjusted, files the hand just taken up under
      // the same name — into the chosen store, which a load does not move.
      this.presetName.set(preset.name);
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'The preset could not be read.'));
    } finally {
      this.loadPending.set(false);
    }
  }

  /** File the form's COERCED values under the typed name in the chosen store (overwrite semantics). */
  protected async save(): Promise<void> {
    const store = this.saveStore();
    if (!this.canSave() || !store) return;
    const preset = this.presetName();
    this.savePending.set(true);
    try {
      await this.core.dispatchData({
        type: 'mountFileWrite',
        mountPointId: store.mountPointId,
        path: presetSettingsPath(this.toolName(), preset),
        content: JSON.stringify(coerceParamValues(this.parameters(), this.values()), null, 2),
      });
      this.toasts.showSuccess(`Preset “${preset}” filed in ${store.mountName}.`);
      // Invalidate rather than refetch this one observer: v4 does, and the key
      // is shared by construction.
      void this.queryClient.invalidateQueries({ queryKey: this.presetsKey() });
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
