import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { map } from 'rxjs';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { WORKSPACE_BACKDROP_REGISTRY, WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import type {
  DocumentStoreSummary,
  ProjectDetail as ProjectDetailDto,
} from '../../core/core-contract';
import { LoadingState } from '../../ui/loading-state';
import { ToastService } from '../../ui/toast.service';
import { GroupStoresCard } from '../groups/group-stores-card';
import { ProjectChatsSection } from './cards/project-chats-section';
import { ProjectCharactersCard } from './cards/project-characters-card';
import { ProjectFilesCard } from './cards/project-files-card';
import { ProjectHeader, type ProjectEditForm } from './cards/project-header';
import { ProjectImageGenerationCard } from './cards/project-image-generation-card';
import { ProjectModelBehaviorCard } from './cards/project-model-behavior-card';
import { ProjectScenariosCard } from './cards/project-scenarios-card';
import { ProjectSettingsCard } from './cards/project-settings-card';
import { ProjectWardrobeCard } from './cards/project-wardrobe-card';
import { resolveFirstVisit } from './project-card-state';
import { PASSIVE_POLL_INTERVAL_MS } from '../salon/story-background.api';
import {
  fetchProject,
  fetchProjectBackground,
  fetchProjectStores,
  projectKeys,
  shouldPassivePollBackground,
  unlinkProjectStore,
  updateProject,
} from './projects.api';

/**
 * The routed project detail (v4 `app/prospero/[id]/ProjectDetailView.tsx`).
 * Route: `/prospero/:id`. A dense responsive card grid over the project's
 * `projectGet`, with per-card expansion memory (all expanded on the first visit,
 * collapsed after — localStorage `quilltap_project_visited_{id}`).
 *
 * Cards: Header, Files (list + thumbnails), Scriptorium (linked stores +
 * unlink), Scenarios (the scope-agnostic manager), Wardrobe (project-tier
 * garments), Characters, Model Behavior, Settings (instructions + state), Image
 * Generation (selects + aesthetic editors), plus the full-width chats section.
 *
 * ## The story background (P4.D92, v4 bug 80 / `c6ff8051`)
 *
 * Inside the workspace the per-view `.qt-page-container::before` layer is
 * suppressed in favour of the ONE arbitrated backdrop
 * (`_workspace.css:108` — v5 carries v4's exact suppression, and so carried its
 * exact defect site). This view therefore REPORTS its resolved background to
 * `WORKSPACE_BACKDROP_REGISTRY` under its own tab id; nothing else reaches the
 * screen. Outside the workspace both tokens are null and the reporter is inert,
 * exactly as v4's hook is a no-op on its legacy route.
 *
 * **Recorded divergence — the `'theme'`-mode fallback is ABSENT.** v4 reports
 * `storyBackgroundUrl || prosperoBackgroundImage || null`
 * (`ProjectDetailView.tsx:98`), falling back to the theme's Prospero subsystem
 * image so the page keeps looking as the list does. v5 has **no
 * subsystem-background machinery at all** — no `useSubsystemInfo` twin, no
 * `subsystem-defaults` transcription, and none of the eight `/images/*.webp`
 * assets v4's defaults name (`public/images/` holds only `avatars/` and
 * `icons/`). That is the standing deferred-loud divergence first recorded at
 * the My Photos lane; bug 80's fallback arm is the newest instance of it, and
 * minting the machinery is explicitly not this lane's business. So in v5
 * `'theme'` mode reports NOTHING and the backdrop is absent — the honest shape,
 * asserted as such in `workspace-project-backdrop-flow.spec.ts`.
 *
 * **Structural difference from v4's fix.** v4 also had to move the projects
 * LIST's subsystem reporter into a `ProsperoListShell` that unmounts while a
 * detail is shown, because the registry keys on the TAB and two live reporters
 * raced (the subsystem won on a deep-linked project tab). v5's list reports
 * nothing at all, so this view is already the single reporter per key — there is
 * no shell to split, and the deep-link case cannot lose.
 */
@Component({
  selector: 'qt-project-detail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    LoadingState,
    ProjectHeader,
    GroupStoresCard,
    ProjectFilesCard,
    ProjectCharactersCard,
    ProjectModelBehaviorCard,
    ProjectSettingsCard,
    ProjectImageGenerationCard,
    ProjectChatsSection,
    ProjectScenariosCard,
    ProjectWardrobeCard,
  ],
  template: `
    @if (projectQuery.isPending()) {
      <qt-loading-state message="Loading project..." class="mt-12" />
    } @else if (projectQuery.isError() || !project()) {
      <div class="flex min-h-[50vh] items-center justify-center">
        <div class="text-center">
          <p class="text-lg qt-text-destructive mb-4">{{ loadErrorMessage() }}</p>
          <button type="button" class="qt-text-primary hover:underline" (click)="goBack()">
            Back to Projects
          </button>
        </div>
      </div>
    } @else {
      <div
        class="qt-page-container text-foreground"
        [style.--story-background-url]="storyBackgroundVar()"
      >
        <qt-project-header
          [project]="project()!"
          [editing]="isEditing()"
          [form]="editForm()"
          [inTab]="embedded()"
          (formChange)="patchForm($event)"
          (editClick)="isEditing.set(true)"
          (cancelEdit)="cancelEdit()"
          (save)="save()"
          (back)="goBack()"
        />

        <div class="mt-6 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4 grid-flow-row-dense">
          <qt-project-files-card [projectId]="id()" [defaultOpen]="firstVisit()" />
          <qt-group-stores-card
            [stores]="stores()"
            [unlinking]="storeUnlinking()"
            (unlink)="onUnlinkStore($event)"
          />

          <qt-project-scenarios-card [projectId]="id()" [defaultOpen]="firstVisit()" />
          <qt-project-wardrobe-card [projectId]="id()" [defaultOpen]="firstVisit()" />

          <qt-project-characters-card [project]="project()!" [defaultOpen]="firstVisit()" />
          <qt-project-model-behavior-card [project]="project()!" [defaultOpen]="firstVisit()" />
          <qt-project-settings-card
            [project]="project()!"
            [form]="editForm()"
            [defaultOpen]="firstVisit()"
            (formChange)="patchForm($event)"
            (save)="save()"
          />
          <qt-project-image-generation-card [project]="project()!" [defaultOpen]="firstVisit()" />
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
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly toasts = inject(ToastService);
  private readonly destroyRef = inject(DestroyRef);
  /**
   * The workspace backdrop seams (v4 `useReportWorkspaceBackdrop` /
   * `useWorkspaceTabId`). Both null ⇒ routed mode, where the per-view
   * `.qt-page-container::before` layer ({@link storyBackgroundVar}) does the
   * painting and reporting is inert — v4's hook is a no-op outside the
   * workspace too.
   */
  private readonly backdropRegistry = inject(WORKSPACE_BACKDROP_REGISTRY, { optional: true });
  private readonly workspaceTabId = inject(WORKSPACE_TAB_ID, { optional: true });

  /**
   * In-tab drill (p4.9j2, v4 `ProsperoView` `selectedProjectId`): when the list
   * renders this detail in place it supplies `projectId` (wins over the route
   * `:id`) and binds `(back)` to restore the list. Null ⇒ routed `/prospero/:id`.
   */
  readonly projectIdInput = input<string | null>(null, { alias: 'projectId' });
  readonly back = output<void>();
  protected readonly embedded = computed(() => this.projectIdInput() != null);

  private readonly routeId = this.route
    ? toSignal(this.route.paramMap.pipe(map((p) => p.get('id') ?? '')), { initialValue: '' })
    : undefined;
  protected readonly id = computed(() => this.projectIdInput() ?? this.routeId?.() ?? '');

  /**
   * First-visit resolves once the id is known (routed: the snapshot at
   * construction; drill: when the `projectId` input lands). `resolveFirstVisit`
   * marks the id visited, so it must run with the REAL id exactly once.
   */
  protected readonly firstVisit = signal(false);

  protected readonly isEditing = signal(false);
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

  /**
   * v4 `useStoryBackground(null, projectId, project?.backgroundDisplayMode !==
   * 'theme')` (`ProjectDetailView.tsx:87-91`): the passive 30s poll is gated on
   * the mode, the FETCH never is. The gate's quirks live in — and are pinned
   * on — {@link shouldPassivePollBackground}.
   */
  protected readonly backgroundPollEnabled = computed(() =>
    shouldPassivePollBackground(this.project()?.backgroundDisplayMode),
  );

  /**
   * The resolved story background. The SERVER decides by display mode (`theme`
   * → null), so there is no client-side mode branch here — exactly as v4 reads
   * the hook's `backgroundUrl` straight out.
   *
   * ⚠ v4-faithful staleness: nothing invalidates this key when the mode is
   * saved (v4 never invalidates `queryKeys.projects.background` anywhere), so a
   * mode change reaches the backdrop on the next fetch — a remount, a window
   * focus, or the 30s poll — not instantly. Carried, not fixed.
   */
  private readonly backgroundQuery = injectQuery(() => ({
    queryKey: projectKeys.background(this.id()),
    enabled: !!this.id(),
    queryFn: () => fetchProjectBackground(this.core, this.id()),
    refetchInterval: this.backgroundPollEnabled() ? PASSIVE_POLL_INTERVAL_MS : false,
    // v4 `refetchOnReconnect: false` (`useStoryBackground.ts:69`).
    refetchOnReconnect: false,
  }));
  protected readonly storyBackgroundUrl = computed<string | null>(
    () => this.backgroundQuery.data()?.backgroundUrl ?? null,
  );

  /**
   * The LEGACY per-view layer's CSS value (v4 `ProjectDetailView.tsx:148-151` —
   * `style={storyBackgroundUrl ? {'--story-background-url': `url('…')`} :
   * undefined}`). v4 keeps this UNCONDITIONALLY alongside the backdrop report,
   * and so does v5: inside `.qt-workspace` the `::before` layer it feeds is
   * `display:none` (`_workspace.css:108`) and the backdrop wins, while on the
   * routed `/prospero/:id` path — which v5 still serves whenever the
   * workspace-tabs flag is off — it is the only route to the screen.
   *
   * Null when there is no background, so the
   * `.qt-page-container:not([style*="--story-background-url"])::before` rule
   * (`_content.css:46`) keeps the layer hidden, exactly as v4's `undefined`
   * style object does.
   */
  protected readonly storyBackgroundVar = computed<string | null>(() => {
    const url = this.storyBackgroundUrl();
    return url ? `url('${url}')` : null;
  });

  constructor() {
    // Resolve first-visit from the real id, exactly once. Routed mode has the
    // snapshot at construction (byte-identical); drill mode waits for the input.
    const routedId = this.route?.snapshot.paramMap.get('id') ?? '';
    if (routedId) {
      this.firstVisit.set(resolveFirstVisit(routedId));
    } else {
      let visitResolved = false;
      effect(() => {
        const pid = this.projectIdInput();
        if (pid && !visitResolved) {
          visitResolved = true;
          this.firstVisit.set(resolveFirstVisit(pid));
        }
      });
    }

    // Report this project's story background to the workspace backdrop (v4 bug
    // 80's fix, `ProjectDetailView.tsx:93-99`). Inside the workspace the per-view
    // ::before layer is suppressed in favour of the ONE arbitrated backdrop, so
    // the background reaches the screen only by this route.
    //
    // v4 reports `storyBackgroundUrl || prosperoBackgroundImage || null`; v5 has
    // no subsystem-background machinery at all (the standing divergence — see
    // the class docs), so the fallback arm is absent and 'theme' mode reports
    // NOTHING rather than the Prospero theme image.
    //
    // Structural difference from v4's fix: v4 also had to move the LIST's
    // subsystem reporter into an unmounting shell, because two live reporters
    // raced over the one tab key. v5's list reports nothing at all, so this is
    // already the single reporter — there is no shell to split.
    const registry = this.backdropRegistry;
    const tabId = this.workspaceTabId;
    if (registry && tabId != null) {
      effect(() => {
        const url = this.storyBackgroundUrl();
        if (url) registry.report(tabId, { url, isSalon: false });
        else registry.clear(tabId);
      });
      // Drilling back to the list (or closing the tab) destroys this view; the
      // entry must go with it or a stale background stays parked under the key.
      this.destroyRef.onDestroy(() => registry.clear(tabId));
    }

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

  protected goBack(): void {
    // Drill mode ⇒ hand control back to the list; routed ⇒ navigate.
    if (this.embedded()) {
      this.back.emit();
      return;
    }
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

  /** v4 `useProjectDetail.ts:63-85` — toast only, no inline surface. */
  protected async save(): Promise<void> {
    const form = this.editForm();
    try {
      await updateProject(this.core, this.id(), {
        name: form.name,
        description: form.description || null,
        instructions: form.instructions || null,
      });
      this.isEditing.set(false);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.id()) });
      this.toasts.showSuccess('Project updated!');
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to update project');
    }
  }

  /**
   * Not a census row (v4's store unlinking lives in a different, unported
   * hook), but retiring the shared `saveError` banner above would otherwise
   * leave this action's failure with no feedback at all — toast it too, for
   * consistency with its siblings in this same component (the same call the
   * group editor made in P4.29 unit 5).
   */
  protected async onUnlinkStore(mountPointId: string): Promise<void> {
    this.storeUnlinking.set(mountPointId);
    try {
      await unlinkProjectStore(this.core, this.id(), mountPointId);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.stores(this.id()) });
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to unlink store');
    } finally {
      this.storeUnlinking.set(null);
    }
  }
}
