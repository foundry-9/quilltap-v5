import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQueryClient } from '@tanstack/angular-query-experimental';

import type { DiagramStyle } from '../../chat/diagrams/diagram-fences';
import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { characterKeys } from './characters.api';

const STYLE_OPTIONS: ReadonlyArray<{ value: DiagramStyle; label: string; hint: string }> = [
  { value: 'handDrawn', label: 'Hand-drawn', hint: 'sketched in ink, a touch wobbly' },
  { value: 'clean', label: 'Clean', hint: 'ruled lines, as the engine drew them' },
];

/** v4 `save-diagram-settings.ts:16-24` — one sentence per change. */
function successMessage(patch: { canDrawDiagrams?: boolean; diagramStyle?: DiagramStyle }): string {
  if (patch.canDrawDiagrams !== undefined) {
    return patch.canDrawDiagrams
      ? 'The pen is theirs — this character may now draw diagrams'
      : 'The pen is put away — this character will draw no more diagrams';
  }
  return patch.diagramStyle === 'clean'
    ? 'Their diagrams will be drawn clean and ruled'
    : 'Their diagrams will be sketched by hand';
}

/**
 * The "this character draws diagrams" card on both character views' Details
 * tabs, with the style choice (hand-drawn / clean) directly beneath it.
 *
 * The style stays visible — and keeps its value — while the ability is off;
 * it is only disabled, so switching the ability off and on again loses
 * nothing.
 *
 * v5 port of v4 `components/characters/CanDrawDiagramsToggle.tsx` +
 * `app/aurora/shared/save-diagram-settings.ts` (`6588972d5`, at 236c04e35).
 * Saved on change, not with the form: one `characterUpdate` dispatch carrying
 * `{ canDrawDiagrams }` or `{ diagramStyle }` (contract C6 — P4.D349 lands the
 * server keys), then the character-detail query is invalidated so the
 * refetch drives the controls. v4's optimistic update + re-fetch on failure
 * collapses to that refetch, as `choose-outfit-card.ts` records for its card.
 */
@Component({
  selector: 'qt-can-draw-diagrams-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'qt-can-draw-diagrams-card' },
  template: `
    <div class="qt-card" data-testid="can-draw-diagrams">
      <label class="flex items-start gap-3 cursor-pointer">
        <input
          type="checkbox"
          class="mt-1 accent-[var(--primary)]"
          [checked]="canDrawDiagrams()"
          [disabled]="saving() || isDisabled()"
          aria-describedby="can-draw-diagrams-hint"
          (change)="onChange($any($event.target).checked)"
        />
        <span class="flex-1 min-w-0">
          <span class="qt-text-label block">Draws diagrams</span>
          <span id="can-draw-diagrams-hint" class="qt-text-small qt-text-secondary block mt-0.5">
            This character may sketch flowcharts, family trees, maps and the like right into the
            conversation. The toggle only grants the pen — to have them reach for it often, say so
            in their prompt.
          </span>
        </span>
        @if (saving()) {
          <span class="h-4 w-4 mt-1 animate-spin rounded-full qt-spinner shrink-0"></span>
        }
      </label>
      <fieldset
        class="mt-3 ml-7"
        [disabled]="styleDisabled()"
        [attr.aria-disabled]="styleDisabled()"
      >
        <legend
          class="qt-text-small mb-1"
          [class]="styleDisabled() ? 'qt-text-muted' : 'qt-text-secondary'"
        >
          Drawing style
        </legend>
        <div class="flex flex-wrap gap-x-6 gap-y-1">
          @for (option of options; track option.value) {
            <label
              class="flex items-center gap-2 text-sm"
              [class]="styleDisabled() ? 'cursor-not-allowed qt-text-muted' : 'cursor-pointer'"
            >
              <input
                type="radio"
                name="diagram-style"
                class="qt-radio"
                [value]="option.value"
                [checked]="diagramStyle() === option.value"
                [disabled]="styleDisabled()"
                (change)="onStyleChange(option.value)"
              />
              <span>
                {{ option.label
                }}<span class="qt-text-xs qt-text-secondary"> — {{ option.hint }}</span>
              </span>
            </label>
          }
        </div>
      </fieldset>
    </div>
  `,
})
export class CanDrawDiagramsCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  /** Null while the character is still loading (v4's `disabled={!character}`). */
  readonly characterId = input.required<string | null>();
  readonly canDrawDiagrams = input(false);
  readonly diagramStyle = input<DiagramStyle>('handDrawn');
  /** The page is read-only (an archived character). */
  readonly disabled = input(false);

  protected readonly options = STYLE_OPTIONS;
  protected readonly saving = signal(false);
  protected readonly isDisabled = computed(() => this.disabled() || this.characterId() === null);
  protected readonly styleDisabled = computed(
    () => !this.canDrawDiagrams() || this.saving() || this.isDisabled(),
  );

  protected onChange(enabled: boolean): Promise<void> {
    return this.save({ canDrawDiagrams: enabled });
  }

  protected onStyleChange(style: DiagramStyle): Promise<void> {
    return this.save({ diagramStyle: style });
  }

  private async save(patch: {
    canDrawDiagrams?: boolean;
    diagramStyle?: DiagramStyle;
  }): Promise<void> {
    const id = this.characterId();
    if (id === null) return;
    this.saving.set(true);
    try {
      await this.core.dispatchData({ type: 'characterUpdate', characterId: id, character: patch });
      this.toasts.showSuccess(successMessage(patch));
    } catch (err) {
      console.error('Failed to save diagram settings', {
        error: err instanceof Error ? err.message : String(err),
      });
      this.toasts.showError(
        err instanceof Error && err.message ? err.message : 'Failed to update diagram settings',
      );
    } finally {
      this.saving.set(false);
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) });
    }
  }
}
