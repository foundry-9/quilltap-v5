import { ChangeDetectionStrategy, Component, effect, inject, input, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { ErrorAlert } from '../../../ui/error-alert';
import { fetchProjectAesthetic, setProjectAesthetic } from '../projects.api';

/**
 * One project aesthetic editor field (v4 `AestheticEditorField`): loads the
 * `lantern`/`aurora` markdown from the project's official store and saves it back.
 *
 * DIVERGENCE (recorded): v4 uses the Lexical editor; v5 ships a plain
 * `<textarea>` with an explicit Save (the Lexical-equivalent is a deferred
 * vertical). The bytes round-trip exactly. An empty save deletes the file
 * (v4 aesthetic-PUT semantics — the server treats empty content as a delete).
 */
@Component({
  selector: 'qt-project-aesthetic-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    <div>
      <label class="qt-text-label block mb-1">{{ label() }}</label>
      <p class="qt-text-xs qt-text-secondary mb-2">{{ description() }}</p>
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" class="mb-2" />
      }
      <textarea
        [value]="content()"
        rows="4"
        [attr.aria-label]="label()"
        class="qt-textarea w-full"
        (input)="content.set($any($event.target).value)"
      ></textarea>
      <div class="mt-2 flex justify-end">
        <button
          type="button"
          class="qt-button qt-button-secondary qt-button-sm"
          [disabled]="saving()"
          (click)="save()"
        >
          {{ saving() ? 'Saving...' : 'Save' }}
        </button>
      </div>
    </div>
  `,
})
export class ProjectAestheticField {
  readonly projectId = input.required<string>();
  readonly kind = input.required<'lantern' | 'aurora'>();
  readonly label = input('');
  readonly description = input('');

  private readonly core = inject(CoreClient);
  protected readonly content = signal('');
  protected readonly saving = signal(false);
  protected readonly saveError = signal<string | null>(null);

  private readonly loadQuery = injectQuery(() => ({
    queryKey: ['projects', 'aesthetic', this.projectId(), this.kind()],
    queryFn: (): Promise<string> => fetchProjectAesthetic(this.core, this.projectId(), this.kind()),
  }));

  constructor() {
    // Seed the textarea from the loaded content once (guard against clobbering
    // keystrokes on re-render).
    let seeded = false;
    effect(() => {
      const loaded = this.loadQuery.data();
      if (loaded !== undefined && !seeded) {
        seeded = true;
        this.content.set(loaded);
      }
    });
  }

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.saveError.set(null);
    try {
      await setProjectAesthetic(this.core, this.projectId(), this.kind(), this.content());
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to save aesthetic');
    } finally {
      this.saving.set(false);
    }
  }
}
