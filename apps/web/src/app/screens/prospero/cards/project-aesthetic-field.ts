import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { AestheticEditorField } from '../../../ui/aesthetic-editor-field';
import { fetchProjectAesthetic, setProjectAesthetic } from '../projects.api';

/**
 * One project aesthetic editor field — v4's `AestheticEditorField` as the project
 * image card calls it (`app/prospero/[id]/components/ImageGenerationCard.tsx:
 * 162-173`): the same component the Images settings tab mounts, pointed at the
 * project's own store instead of the Quilltap General one.
 *
 * All of the behaviour (the load gate, the editor, the dirty-gated Save, the
 * "Saved" span) lives in the shared {@link AestheticEditorField}, exactly as it
 * does in v4. This wrapper is only the project-verb wiring: v4 passes a
 * `loadUrl`, and the dispatch boundary has no URL to pass, so it injects the
 * `projectAestheticGet`/`Set` callbacks instead. An empty save deletes the file
 * (the aesthetic-PUT semantics, pinned by lane A's differential — the server
 * owns them).
 */
@Component({
  selector: 'qt-project-aesthetic-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [AestheticEditorField],
  template: `
    <qt-aesthetic-editor-field
      [label]="label()"
      [description]="description()"
      [queryKey]="queryKey()"
      [load]="load"
      [save]="save"
    />
  `,
})
export class ProjectAestheticField {
  readonly projectId = input.required<string>();
  readonly kind = input.required<'lantern' | 'aurora'>();
  readonly label = input('');
  readonly description = input('');

  private readonly core = inject(CoreClient);

  protected readonly queryKey = computed(
    () => ['projects', 'aesthetic', this.projectId(), this.kind()] as const,
  );

  // Bound as stable arrow properties: a fresh closure each render would churn
  // the query's identity.
  protected readonly load = (): Promise<string> =>
    fetchProjectAesthetic(this.core, this.projectId(), this.kind());

  protected readonly save = (content: string): Promise<void> =>
    setProjectAesthetic(this.core, this.projectId(), this.kind(), content);
}
