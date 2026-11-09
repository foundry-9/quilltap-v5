import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  type TemplateRef,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { pickPrimaryProjectStore } from '../chat/library-picker/library-picker.model';
import { CoreClient } from '../core/core-client';
import { fetchProjectStores } from './document-store.api';
import { projectStoreKeys } from './document-store.keys';
import type { DocumentEntry } from './document-store.source';
import { DocumentStoreBrowser, type DocumentBrowserFooterContext } from './document-store-browser';

/**
 * v4 `useProjectStoreId(projectId)` (`ProjectStoreBrowser.tsx:24-34`): the
 * id of a project's own store, once known — the server's
 * `officialMountPointId` (C14), else `pickPrimaryProjectStore` over the
 * linked stores, else null. Call in an injection context.
 */
export function injectProjectStoreId(projectId: () => string | null) {
  const core = inject(CoreClient);
  const query = injectQuery(() => ({
    queryKey: projectStoreKeys.stores(projectId() ?? ''),
    queryFn: () => fetchProjectStores(core, projectId() ?? ''),
    enabled: !!projectId(),
  }));
  return {
    storeId: computed(() => {
      const data = query.data();
      return data
        ? (data.officialMountPointId ?? pickPrimaryProjectStore(data.mountPoints)?.id ?? null)
        : null;
    }),
    isLoading: computed(() => !!projectId() && query.isLoading()),
    isError: computed(() => query.isError()),
  };
}

/**
 * A project's own document store, browsed — a port of v4
 * `components/document-browser/ProjectStoreBrowser.tsx` (`c17033f04`, pin
 * `624d0d25c`; P4.D387 item 13): the store the project names as official
 * (C14), the primary linked one otherwise, and v4's three texts while it is
 * found, unreachable or absent. Every other input passes through to the
 * store browser. The read lives in `document-store.api.ts` (ruling L-5 — no
 * edit to `screens/prospero/projects.api.ts`).
 */
@Component({
  selector: 'qt-project-store-browser',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DocumentStoreBrowser],
  template: `
    @if (project.isLoading()) {
      <p class="qt-text-small qt-text-muted p-4">Finding the project's shelves…</p>
    } @else if (project.isError() || !project.storeId()) {
      <p class="qt-text-small qt-text-muted p-4">
        {{
          project.isError()
            ? 'The project’s document store could not be reached just now.'
            : 'This project has no document store of its own.'
        }}
      </p>
    } @else {
      @for (id of [project.storeId()!]; track id) {
        <qt-document-store-browser
          [storeId]="id"
          [scrollList]="scrollList()"
          [pickMode]="pickMode()"
          [footer]="footer()"
          [opensStores]="opensStores()"
          (pickFile)="pickFile.emit($event)"
          (openStore)="openStore.emit($event)"
        />
      }
    }
  `,
})
export class ProjectStoreBrowser {
  readonly projectId = input.required<string>();
  readonly scrollList = input(false);
  readonly pickMode = input(false);
  readonly footer = input<TemplateRef<DocumentBrowserFooterContext> | undefined>(undefined);
  readonly opensStores = input(false);
  readonly pickFile = output<DocumentEntry>();
  readonly openStore = output<string>();

  protected readonly project = injectProjectStoreId(() => this.projectId());
}
