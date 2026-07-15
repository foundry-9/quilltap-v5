import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { MarkdownField } from '../../../editor/markdown-field';
import { ErrorAlert } from '../../../ui/error-alert';
import { fetchProjectAesthetic, setProjectAesthetic } from '../projects.api';

/**
 * One project aesthetic editor field (v4 `AestheticEditorField`): loads the
 * `lantern`/`aurora` markdown from the project's official store and saves it back.
 * The body is a `qt-markdown-field` (v4 `:119`, its `minHeight="8rem"` and
 * `ariaLabel={label}`). An empty save deletes the file (v4 aesthetic-PUT
 * semantics — the server treats empty content as a delete).
 *
 * The editor renders only once the load has settled — v4's own
 * `loading ? 'Loading…' : <editor>` gate (`:117-119`), and load-bearing here.
 * v4 never emits on a load: its mount-time parse is tagged `external-sync` and
 * its update listener skips that tag (`MarkdownBridgePlugin.tsx:168-181`), which
 * is why v4 can bump `remountKey` after each load without dirtying the form.
 * v5's equivalent is the field's absorb-once seam, and that swallows exactly one
 * emit — so the content must be in hand BEFORE the editor mounts. Were the
 * editor to mount empty and take the content later, the load would surface as an
 * edit and a plain Save would rewrite the stored file in normalized bytes.
 * Seeding inside `queryFn` (the {@link CharacterAppearanceTab} pattern)
 * guarantees that ordering.
 */
@Component({
  selector: 'qt-project-aesthetic-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, MarkdownField],
  template: `
    <div>
      <label class="qt-text-label block mb-1">{{ label() }}</label>
      <p class="qt-text-xs qt-text-secondary mb-2">{{ description() }}</p>
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" class="mb-2" />
      }
      @if (loadQuery.isPending()) {
        <div class="qt-text-secondary qt-text-small py-4">Loading…</div>
      } @else {
        <qt-markdown-field
          minHeight="8rem"
          [ariaLabel]="label()"
          [value]="content()"
          (contentChange)="content.set($event)"
        />
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
      }
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

  // Seed from inside queryFn, so `content` is already set the moment the query
  // leaves the pending state and the editor mounts (see the class doc — the
  // editor must never mount ahead of its content).
  protected readonly loadQuery = injectQuery(() => ({
    queryKey: ['projects', 'aesthetic', this.projectId(), this.kind()],
    queryFn: async (): Promise<string> => {
      const loaded = await fetchProjectAesthetic(this.core, this.projectId(), this.kind());
      this.content.set(loaded);
      return loaded;
    },
  }));

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
