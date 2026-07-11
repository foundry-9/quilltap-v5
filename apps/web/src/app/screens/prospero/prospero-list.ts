import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { ErrorAlert } from '../../ui/error-alert';
import { LoadingState } from '../../ui/loading-state';
import { ProjectCard } from './project-card';
import { ProjectCreateDialog } from './project-create-dialog';
import { ProjectDeleteDialog } from './project-delete-dialog';
import { deleteProject, fetchProjects, projectKeys, type ProjectCardModel } from './projects.api';

/**
 * The Projects (Prospero) list (v4 `app/prospero/ProsperoView.tsx`): a title +
 * Create Project button, a 1/2/3-col card grid over the `projectList` dispatch
 * (createdAt desc, no search/sort), an empty state, and a delete-with-confirm
 * flow. Card click / post-create navigate into the routed detail (`/prospero/:id`).
 */
@Component({
  selector: 'qt-prospero-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingState, ErrorAlert, ProjectCard, ProjectCreateDialog, ProjectDeleteDialog],
  template: `
    <div class="qt-page-container text-foreground">
      <div
        class="flex flex-wrap items-center justify-between gap-4 border-b qt-border-default/60 pb-6"
      >
        <h1 class="qt-heading-1 leading-tight">Projects</h1>
        <button
          type="button"
          class="qt-button inline-flex items-center rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-md transition hover:qt-bg-primary/90"
          (click)="createOpen.set(true)"
        >
          Create Project
        </button>
      </div>

      @if (projectsQuery.isPending()) {
        <qt-loading-state message="Loading projects..." class="mt-12" />
      } @else if (projectsQuery.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          class="mt-8"
          (retry)="projectsQuery.refetch()"
        />
      } @else {
        @if (deleteError(); as msg) {
          <qt-error-alert [message]="msg" class="mt-6" />
        }
        @if (projects().length === 0) {
          <div
            class="mt-12 rounded-2xl border border-dashed qt-border-default/70 qt-bg-card/80 px-8 py-12 text-center qt-shadow-sm"
          >
            <p class="mb-4 text-lg qt-text-secondary">No projects yet</p>
            <button
              type="button"
              class="qt-text-primary hover:text-primary/80"
              (click)="createOpen.set(true)"
            >
              Create your first project
            </button>
          </div>
        } @else {
          <div class="mt-8 grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
            @for (project of projects(); track project.id) {
              <qt-project-card
                [project]="project"
                (open)="openProject($event)"
                (delete)="deleteTarget.set($event)"
              />
            }
          </div>
        }
      }
    </div>

    @if (createOpen()) {
      <qt-project-create-dialog (close)="createOpen.set(false)" (created)="onCreated($event)" />
    }

    @if (deleteTarget()) {
      <qt-project-delete-dialog (close)="deleteTarget.set(null)" (confirm)="onDeleteConfirm()" />
    }
  `,
})
export class ProsperoList {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly router = inject(Router);

  protected readonly createOpen = signal(false);
  protected readonly deleteTarget = signal<string | null>(null);
  protected readonly deleteError = signal<string | null>(null);

  protected readonly projectsQuery = injectQuery(() => ({
    queryKey: projectKeys.list(),
    queryFn: (): Promise<ProjectCardModel[]> => fetchProjects(this.core),
  }));

  protected readonly projects = computed(() => this.projectsQuery.data() ?? []);

  protected errorMessage(): string {
    const err = this.projectsQuery.error();
    return err instanceof Error ? err.message : 'An error occurred';
  }

  protected openProject(id: string): void {
    void this.router.navigate(['/prospero', id]);
  }

  protected onCreated(id: string): void {
    this.createOpen.set(false);
    void this.queryClient.invalidateQueries({ queryKey: projectKeys.list() });
    void this.router.navigate(['/prospero', id]);
  }

  protected async onDeleteConfirm(): Promise<void> {
    const id = this.deleteTarget();
    if (!id) {
      return;
    }
    this.deleteError.set(null);
    const key = projectKeys.list();
    const previous = this.queryClient.getQueryData<ProjectCardModel[]>(key);
    this.queryClient.setQueryData<ProjectCardModel[]>(key, (prev) =>
      (prev ?? []).filter((p) => p.id !== id),
    );
    this.deleteTarget.set(null);
    try {
      await deleteProject(this.core, id);
    } catch (err) {
      this.queryClient.setQueryData<ProjectCardModel[]>(key, previous);
      this.deleteError.set(err instanceof Error ? err.message : 'Failed to delete project');
    } finally {
      await this.queryClient.invalidateQueries({ queryKey: key });
    }
  }
}
