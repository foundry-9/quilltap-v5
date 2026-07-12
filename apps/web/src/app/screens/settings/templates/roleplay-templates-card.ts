import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto, RoleplayTemplateDto } from '../../../core/core-contract';
import { EmptyState } from '../../../ui/empty-state';
import { ErrorAlert } from '../../../ui/error-alert';
import { LoadingState } from '../../../ui/loading-state';
import { TemplateFormModal } from './template-form-modal';
import { TemplatePreviewModal } from './template-preview-modal';
import {
  deleteRoleplayTemplate,
  fetchRoleplayTemplates,
  templateKeys,
} from './templates.api';

/**
 * The Roleplay Templates manager (v4 `components/settings/roleplay-templates/
 * index.tsx`): the global Default Template selector, the read-only Built-in
 * grid (Preview + Copy as New), and the My Templates grid (Create / Preview /
 * Edit / Delete-with-confirm). Built-ins never expose Edit/Delete (the server
 * also 403s — surfaced verbatim). Duplicate-name 409s surface verbatim. Copy
 * carries over from v4.
 *
 * NOTE: the global Default Template rides the live `chatSettings` /
 * `chatSettingsUpdate` surface writing `defaultRoleplayTemplateId`; that field's
 * presence in the P4.6d chat-settings row is confirmed at unification (it is a
 * v4 `settings/chat` field). The "Draft formatting instructions" helper is a
 * named deferral.
 */
@Component({
  selector: 'qt-roleplay-templates-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingState, ErrorAlert, EmptyState, TemplateFormModal, TemplatePreviewModal],
  template: `
    @if (templatesQuery.isPending()) {
      <qt-loading-state message="Loading templates..." />
    } @else {
      @if (templatesQuery.isError()) {
        <qt-error-alert
          [message]="errorMessage()"
          [retryable]="true"
          class="mb-4"
          (retry)="templatesQuery.refetch()"
        />
      }
      @if (banner(); as msg) {
        <div class="qt-alert-error mb-4">{{ msg }}</div>
      }

      <!-- Default Template (global chat setting) -->
      <section class="qt-card p-4 mb-6">
        <h3 class="qt-text-section mb-1">Default Template</h3>
        <p class="qt-text-xs qt-text-secondary mb-3">
          This template will be applied to all new chats by default. You can override it
          per-character or per-chat.
        </p>
        <label class="block qt-text-label mb-1">Template for New Chats</label>
        <div class="flex items-center gap-3">
          <select
            class="qt-select max-w-md"
            [disabled]="defaultSaving()"
            (change)="onDefaultChange($any($event.target).value)"
          >
            <option value="" [selected]="defaultTemplateId() === ''">
              None (no formatting template)
            </option>
            @for (t of templates(); track t.id) {
              <option [value]="t.id" [selected]="defaultTemplateId() === t.id">
                {{ t.name }}{{ t.isBuiltIn ? ' (Built-in)' : '' }}
              </option>
            }
          </select>
          @if (defaultSaving()) {
            <span class="qt-text-small">Saving...</span>
          }
        </div>
      </section>

      <!-- Built-in Templates -->
      <section class="mb-6">
        <h3 class="qt-text-section mb-1">Built-in Templates</h3>
        <p class="qt-text-xs qt-text-secondary mb-3">
          These templates are provided by Quilltap and cannot be modified. You can copy them to
          create your own version.
        </p>
        @if (builtInTemplates().length === 0) {
          <p class="qt-text-xs qt-text-muted italic">No built-in templates available.</p>
        } @else {
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            @for (t of builtInTemplates(); track t.id) {
              <div class="qt-card p-4">
                <div class="flex items-start justify-between gap-2">
                  <div class="min-w-0">
                    <h4 class="font-medium truncate">{{ t.name }}</h4>
                    @if (t.description) {
                      <p class="qt-text-xs qt-text-muted line-clamp-2">{{ t.description }}</p>
                    }
                  </div>
                  <span class="qt-badge-chat px-2 py-0.5 rounded qt-text-label-xs flex-shrink-0"
                    >Built-in</span
                  >
                </div>
                <div class="flex gap-2 mt-3">
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    (click)="openPreview(t)"
                  >
                    Preview
                  </button>
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    (click)="openCopy(t)"
                  >
                    Copy as New
                  </button>
                </div>
              </div>
            }
          </div>
        }
      </section>

      <!-- My Templates -->
      <section>
        <div class="flex items-center justify-between mb-1">
          <h3 class="qt-text-section">My Templates</h3>
          <button type="button" class="qt-button-primary qt-button-sm" (click)="openCreate()">
            Create Template
          </button>
        </div>
        <p class="qt-text-xs qt-text-secondary mb-3">
          Custom templates you've created for your roleplay sessions.
        </p>
        @if (userTemplates().length === 0) {
          <qt-empty-state
            variant="dashed"
            title="No custom templates yet"
            description="Create one to define your own roleplay formatting style."
            actionLabel="Create Template"
            (action)="openCreate()"
          />
        } @else {
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            @for (t of userTemplates(); track t.id) {
              <div class="qt-card p-4">
                <div class="min-w-0">
                  <h4 class="font-medium truncate">{{ t.name }}</h4>
                  @if (t.description) {
                    <p class="qt-text-xs qt-text-muted line-clamp-2">{{ t.description }}</p>
                  }
                </div>
                <div class="flex gap-2 mt-3">
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    (click)="openPreview(t)"
                  >
                    Preview
                  </button>
                  <button
                    type="button"
                    class="qt-button-secondary qt-button-sm"
                    (click)="openEdit(t)"
                  >
                    Edit
                  </button>
                  @if (deleteConfirmId() === t.id) {
                    <button
                      type="button"
                      class="qt-button-destructive qt-button-sm"
                      [disabled]="deletingId() === t.id"
                      (click)="doDelete(t.id)"
                    >
                      {{ deletingId() === t.id ? 'Deleting...' : 'Confirm' }}
                    </button>
                    <button
                      type="button"
                      class="qt-button-ghost qt-button-sm"
                      (click)="deleteConfirmId.set(null)"
                    >
                      Cancel
                    </button>
                  } @else {
                    <button
                      type="button"
                      class="qt-button-ghost qt-button-sm qt-text-destructive"
                      (click)="deleteConfirmId.set(t.id)"
                    >
                      Delete
                    </button>
                  }
                </div>
              </div>
            }
          </div>
        }
      </section>
    }

    @if (formOpen()) {
      <qt-template-form-modal
        [template]="formTemplate()"
        [copy]="formCopy()"
        (close)="formOpen.set(false)"
        (saved)="onSaved()"
      />
    }
    @if (previewTemplate(); as t) {
      <qt-template-preview-modal
        [template]="t"
        (close)="previewTemplate.set(null)"
        (copyAsNew)="openCopy(t)"
      />
    }
  `,
})
export class RoleplayTemplatesCard {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly formOpen = signal(false);
  protected readonly formTemplate = signal<RoleplayTemplateDto | null>(null);
  protected readonly formCopy = signal(false);
  protected readonly previewTemplate = signal<RoleplayTemplateDto | null>(null);
  protected readonly deleteConfirmId = signal<string | null>(null);
  protected readonly deletingId = signal<string | null>(null);
  protected readonly defaultSaving = signal(false);
  protected readonly banner = signal<string | null>(null);

  protected readonly templatesQuery = injectQuery(() => ({
    queryKey: templateKeys.list(),
    queryFn: (): Promise<RoleplayTemplateDto[]> => fetchRoleplayTemplates(this.core),
  }));

  private readonly settingsQuery = injectQuery(() => ({
    queryKey: ['chat-settings'],
    queryFn: async (): Promise<ChatSettingsDto> => {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      return resp.data;
    },
  }));

  protected readonly templates = computed(() => this.templatesQuery.data() ?? []);
  protected readonly builtInTemplates = computed(() => this.templates().filter((t) => t.isBuiltIn));
  protected readonly userTemplates = computed(() => this.templates().filter((t) => !t.isBuiltIn));

  protected readonly defaultTemplateId = computed(() => {
    const v = this.settingsQuery.data()?.['defaultRoleplayTemplateId'];
    return typeof v === 'string' ? v : '';
  });

  protected errorMessage(): string {
    const err = this.templatesQuery.error();
    return err instanceof Error ? err.message : 'Failed to load templates';
  }

  protected openCreate(): void {
    this.formTemplate.set(null);
    this.formCopy.set(false);
    this.formOpen.set(true);
  }

  protected openEdit(t: RoleplayTemplateDto): void {
    this.formTemplate.set(t);
    this.formCopy.set(false);
    this.formOpen.set(true);
  }

  protected openCopy(t: RoleplayTemplateDto): void {
    this.previewTemplate.set(null);
    this.formTemplate.set(t);
    this.formCopy.set(true);
    this.formOpen.set(true);
  }

  protected openPreview(t: RoleplayTemplateDto): void {
    this.previewTemplate.set(t);
  }

  protected onSaved(): void {
    this.formOpen.set(false);
    this.banner.set(null);
    void this.queryClient.invalidateQueries({ queryKey: templateKeys.list() });
  }

  protected async doDelete(id: string): Promise<void> {
    this.deletingId.set(id);
    this.banner.set(null);
    try {
      await deleteRoleplayTemplate(this.core, id);
      this.deleteConfirmId.set(null);
      await this.queryClient.invalidateQueries({ queryKey: templateKeys.list() });
    } catch (err) {
      // The 403 built-in guard / any server error surfaces in the banner.
      this.banner.set(err instanceof Error && err.message ? err.message : 'An error occurred');
    } finally {
      this.deletingId.set(null);
    }
  }

  protected async onDefaultChange(value: string): Promise<void> {
    this.defaultSaving.set(true);
    this.banner.set(null);
    try {
      await this.core.dispatchData({
        type: 'chatSettingsUpdate',
        settings: { defaultRoleplayTemplateId: value || null },
      });
      await this.queryClient.invalidateQueries({ queryKey: ['chat-settings'] });
    } catch (err) {
      this.banner.set(
        err instanceof Error && err.message ? err.message : 'Failed to update default template',
      );
    } finally {
      this.defaultSaving.set(false);
    }
  }
}
