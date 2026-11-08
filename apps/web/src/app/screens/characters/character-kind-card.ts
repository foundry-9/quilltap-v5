import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  type ElementRef,
  linkedSignal,
  signal,
  viewChildren,
} from '@angular/core';
import { injectQueryClient } from '@tanstack/angular-query-experimental';

import { type CharacterKind, resolveCharacterKind } from '../../core/character-kind';
import { CoreDispatchError } from '../../core/core-contract';
import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { characterKeys } from './characters.api';

const KIND_OPTIONS: ReadonlyArray<{ value: CharacterKind; label: string; hint: string }> = [
  {
    value: 'roleplay',
    label: 'Roleplay character',
    hint: 'a person in the story, with stakes of their own, who feels what happens and acts on it',
  },
  {
    value: 'assistant',
    label: 'Assistant',
    hint: 'a capable aide whose business is doing work for you: research, drafting, keeping the books',
  },
];

/** v4 `app/aurora/shared/save-character-kind.ts:11-14`. */
const SUCCESS_MESSAGES: Record<CharacterKind, string> = {
  roleplay: 'Very good — this character takes the stage as a player in the drama',
  assistant: 'Very good — this character joins the staff as an assistant',
};

/** v4 `save-character-kind.ts` throws this FIXED sentence on any refusal. */
const FAILURE_MESSAGE = 'Failed to update the kind of character';

/** v4's `useId()`: one radio-group name per mounted card. */
let seq = 0;

/**
 * CharacterKindCard
 *
 * The "how is this character performed" card shared by the character detail
 * and edit views' Details tabs: roleplay (a person inside a story, the
 * default) or assistant (a character whose job is to do work for you).
 *
 * The radio group's name and the hint's id are per instance (v4 `useId`): the
 * workspace keeps the view and edit tabs mounted side by side, and two groups
 * sharing one name in a document would act as a single group.
 *
 * v5 port of v4 `components/characters/CharacterKindPicker.tsx` +
 * `app/aurora/shared/save-character-kind.ts` + the views' `handleSaveKind`
 * (`b09ca46bb`, at the pin `624d0d25c`). Saved on change with one
 * `characterUpdate {kind}` dispatch (contract C5). OPTIMISTIC, as v4: the
 * choice shows at once through a `linkedSignal` seeded from the `kind` input
 * (which also follows a refetch); a failed save snaps it back to the stored
 * kind and re-fetches the character. Success does not refetch (v4 does not).
 * Every read of the raw wire value goes through `resolveCharacterKind`.
 */
@Component({
  selector: 'qt-character-kind-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  // An unruled custom element is `display: inline`; the host style keeps it a
  // block (no `qt-*` host class — `check-qt-classes` reds one with no rule).
  styles: [':host { display: block; }'],
  template: `
    <div class="qt-card" data-testid="character-kind">
      <fieldset [disabled]="inert()" [attr.aria-describedby]="hintId">
        <legend class="qt-text-label flex items-center gap-3">
          Kind of character
          @if (saving()) {
            <span class="h-4 w-4 animate-spin rounded-full qt-spinner shrink-0"></span>
          }
        </legend>
        <p [id]="hintId" class="qt-text-small qt-text-secondary mt-0.5 mb-2">
          Whether this character is a player in the drama or a member of the staff. It decides how
          they are directed when the curtain is up.
        </p>
        <div class="flex flex-col gap-1">
          @for (option of options; track option.value) {
            <label
              class="flex items-center gap-2 text-sm"
              [class.cursor-not-allowed]="inert()"
              [class.qt-text-muted]="inert()"
              [class.cursor-pointer]="!inert()"
            >
              <input
                #radio
                type="radio"
                [name]="groupName"
                class="qt-radio"
                [value]="option.value"
                [checked]="selected() === option.value"
                [disabled]="inert()"
                (change)="onChange(option.value)"
              />
              <span
                >{{ option.label
                }}<span class="qt-text-xs qt-text-secondary"> — {{ option.hint }}</span></span
              >
            </label>
          }
        </div>
      </fieldset>
    </div>
  `,
})
export class CharacterKindCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  /** Null while the character is still loading (v4's `disabled={!character}`). */
  readonly characterId = input.required<string | null>();
  /** The raw wire value — anything a v4 store holds. */
  readonly kind = input<unknown>(undefined);
  /** The page is read-only (an archived character). */
  readonly disabled = input(false);

  protected readonly options = KIND_OPTIONS;
  protected readonly groupName = `qt-character-kind-${++seq}`;
  protected readonly hintId = `${this.groupName}-hint`;
  protected readonly saving = signal(false);
  protected readonly selected = linkedSignal(() => resolveCharacterKind({ kind: this.kind() }));
  private readonly radios = viewChildren<ElementRef<HTMLInputElement>>('radio');
  protected readonly inert = computed(
    () => this.saving() || this.disabled() || this.characterId() === null,
  );

  protected async onChange(kind: CharacterKind): Promise<void> {
    const id = this.characterId();
    if (id === null) return;
    this.selected.set(kind);
    this.saving.set(true);
    try {
      await this.core.dispatchData({ type: 'characterUpdate', characterId: id, character: { kind } });
      this.toasts.showSuccess(SUCCESS_MESSAGES[kind]);
    } catch (err) {
      console.error('Failed to save character kind', {
        error: err instanceof Error ? err.message : String(err),
      });
      this.toasts.showError(
        err instanceof CoreDispatchError
          ? FAILURE_MESSAGE
          : err instanceof Error && err.message
            ? err.message
            : FAILURE_MESSAGE,
      );
      // Snap the picker back (v4 re-fetches). An unchanged `kind` input never
      // re-runs the link, and with no render between the optimistic set and
      // this one the `[checked]` binding sees no change — so the DOM is
      // written too (dogfood #143's class).
      const stored = resolveCharacterKind({ kind: this.kind() });
      this.selected.set(stored);
      for (const r of this.radios()) r.nativeElement.checked = r.nativeElement.value === stored;
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) });
    } finally {
      this.saving.set(false);
    }
  }
}
