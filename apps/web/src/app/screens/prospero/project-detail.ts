import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { map } from 'rxjs';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type {
  DocumentStoreSummary,
  ProjectDetail as ProjectDetailDto,
} from '../../core/core-contract';
import { ErrorAlert } from '../../ui/error-alert';
import { LoadingState } from '../../ui/loading-state';
import { GroupStoresCard } from '../groups/group-stores-card';
import { ProjectChatsSection } from './cards/project-chats-section';
import { ProjectCharactersCard } from './cards/project-characters-card';
import { ProjectHeader, type ProjectEditForm } from './cards/project-header';
import { ProjectModelBehaviorCard } from './cards/project-model-behavior-card';
import { ProjectSettingsCard } from './cards/project-settings-card';
import { resolveFirstVisit } from './project-card-state';
import {
  fetchProject,
  fetchProjectStores,
  projectKeys,
  unlinkProjectStore,
  updateProject,
} from './projects.api';

/**
 * The routed project detail (v4 `app/prospero/[id]/ProjectDetailView.tsx`).
 * Route: `/prospero/:id`. A dense responsive card grid over the project's
 * `projectGet`, with per-card expansion memory (all expanded on the first visit,
 * collapsed after — localStorage `quilltap_project_visited_{id}`).
 *
 * Tier-1 cards: Header, Scriptorium (linked stores + unlink), Characters, Model
 * Behavior, Settings (instructions + state), plus the full-width chats section.
 * Files / Scenarios / Wardrobe / Image Generation land in the tier-2 slice.
 */
@Component({
  selector: 'qt-project-detail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    LoadingState,
    ErrorAlert,
    ProjectHeader,
    GroupStoresCard,
    ProjectCharactersCard,
    ProjectModelBehaviorCard,
    ProjectSettingsCard,
    ProjectChatsSection,
  ],
  template: `
    @if (projectQuery.isPending()) {
      <qt-loading-state message="Loading project..." class="mt-12" />
    } @else if (projectQuery.isError() || !project()) {
      <div class="flex min-h-[50vh] items-center justify-center">
        <div class="text-center">
          <p class="text-lg qt-text-destructive mb-4">{{ loadErrorMessage() }}</p>
          <button type="button" class="qt-text-primary hover:underline" (click)="back()">
            Back to Projects
          </button>
        </div>
      </div>
    } @else {
      <div class="qt-page-container text-foreground">
        <qt-project-header
          [project]="project()!"
          [editing]="isEditing()"
          [form]="editForm()"
          (formChange)="patchForm($event)"
          (editClick)="isEditing.set(true)"
          (cancelEdit)="cancelEdit()"
          (save)="save()"
        />

        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mt-4" />
        }

        <div class="mt-6 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4 grid-flow-row-dense">
          <qt-group-stores-card
            [stores]="stores()"
            [unlinking]="storeUnlinking()"
            (unlink)="onUnlinkStore($event)"
          />
          <qt-project-characters-card [project]="project()!" [defaultOpen]="firstVisit" />
          <qt-project-model-behavior-card [project]="project()!" [defaultOpen]="firstVisit" />
          <qt-project-settings-card
            [project]="project()!"
            [form]="editForm()"
            [defaultOpen]="firstVisit"
            (formChange)="patchForm($event)"
            (save)="save()"
          />
        </div>

        <qt-project-chats-section [projectId]="id()" />
      </div>
    }
  `,
})
export class ProjectDetailScreen {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);

  protected readonly id = toSignal(this.route.paramMap.pipe(map((p) => p.get('id') ?? '')), {
    initialValue: '',
  });

  /** First-visit resolves once at construction (before the cards mount). */
  protected readonly firstVisit = resolveFirstVisit(this.route.snapshot.paramMap.get('id') ?? '');

  protected readonly isEditing = signal(false);
  protected readonly saveError = signal<string | null>(null);
  protected readonly editForm = signal<ProjectEditForm>({
    name: '',
    description: '',
    instructions: '',
  });
  protected readonly storeUnlinking = signal<string | null>(null);

  protected readonly projectQuery = injectQuery(() => ({
    queryKey: projectKeys.detail(this.id()),
    queryFn: (): Promise<ProjectDetailDto> => fetchProject(this.core, this.id()),
  }));

  private readonly storesQuery = injectQuery(() => ({
    queryKey: projectKeys.stores(this.id()),
    queryFn: (): Promise<DocumentStoreSummary[]> => fetchProjectStores(this.core, this.id()),
  }));

  protected readonly project = computed(() => this.projectQuery.data() ?? null);
  protected readonly stores = computed(() => this.storesQuery.data() ?? []);

  constructor() {
    // Seed the edit form from the loaded project ONCE per id (v4 sets editForm in
    // fetchProject). Guard so keystrokes aren't clobbered by re-renders.
    let seededFor: string | null = null;
    effect(() => {
      const project = this.projectQuery.data();
      if (project && seededFor !== project.id) {
        seededFor = project.id;
        this.editForm.set({
          name: project.name ?? '',
          description: project.description ?? '',
          instructions: project.instructions ?? '',
        });
      }
    });
  }

  protected loadErrorMessage(): string {
    const err = this.projectQuery.error();
    return err instanceof Error ? err.message : 'Project not found';
  }

  protected back(): void {
    void this.router.navigate(['/prospero']);
  }

  protected patchForm(patch: Partial<ProjectEditForm>): void {
    this.editForm.update((f) => ({ ...f, ...patch }));
  }

  protected cancelEdit(): void {
    const project = this.project();
    if (project) {
      this.editForm.set({
        name: project.name ?? '',
        description: project.description ?? '',
        instructions: project.instructions ?? '',
      });
    }
    this.isEditing.set(false);
  }

  protected async save(): Promise<void> {
    this.saveError.set(null);
    const form = this.editForm();
    try {
      await updateProject(this.core, this.id(), {
        name: form.name,
        description: form.description || null,
        instructions: form.instructions || null,
      });
      this.isEditing.set(false);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.id()) });
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to update project');
    }
  }

  protected async onUnlinkStore(mountPointId: string): Promise<void> {
    this.storeUnlinking.set(mountPointId);
    try {
      await unlinkProjectStore(this.core, this.id(), mountPointId);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.stores(this.id()) });
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to unlink store');
    } finally {
      this.storeUnlinking.set(null);
    }
  }
}
