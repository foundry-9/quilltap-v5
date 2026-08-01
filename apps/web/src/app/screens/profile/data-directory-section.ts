import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { Icon } from '../../ui/icon';
import { fetchDataDir, profileKeys } from './profile.api';
import { ToastService } from '../../ui/toast.service';

/** v4 `DataDirectorySection.tsx:24-29` — the platform display names, verbatim. */
const PLATFORM_NAMES: Record<string, string> = {
  docker: 'Docker',
  linux: 'Linux',
  darwin: 'macOS',
  win32: 'Windows',
};

/**
 * Where this instance keeps its data (v4
 * `components/profile/DataDirectorySection.tsx`): the path (copyable), the
 * configuration source, the platform, and — in v4 — a button that opens the
 * folder in the system file browser.
 *
 * ## The open action
 *
 * v4 renders the button when `canOpen` (`!isDocker`) and otherwise renders an
 * explanatory box. **v5 has no opener at all**: `POST ?action=open` is
 * refusal-armed server-side (a Tauri shell-open is a named future native
 * nicety), so this component always renders the box form rather than a button
 * that would fail.
 *
 * `canOpen` still arrives faithfully from the server — it describes the
 * ENVIRONMENT, and when it is false because we really are in Docker, v4's own
 * Docker copy is the right thing to show and is carried verbatim. The
 * non-Docker case gets its own honest sentence rather than v4's Docker one.
 */
@Component({
  selector: 'qt-data-directory-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-card">
      <div class="qt-card-header">
        <h2 class="text-xl font-semibold">Data Directory</h2>
        @if (dirInfo()) {
          <p class="qt-text-muted text-sm mt-1">Where Quilltap stores your data, files, and logs</p>
        }
      </div>
      <div class="qt-card-content">
        @if (dirQuery.isPending()) {
          <div class="qt-text-muted text-sm">Loading...</div>
        } @else if (dirInfo(); as info) {
          <div class="space-y-4">
            <!-- Path display -->
            <div>
              <div class="qt-text-label text-sm mb-1">Location</div>
              <div class="flex items-center gap-2">
                <code
                  class="qt-text-primary font-mono text-sm qt-bg-muted/50 px-2 py-1 rounded flex-1 overflow-x-auto"
                  data-testid="data-dir-path"
                  >{{ info.path }}</code
                >
                <button
                  type="button"
                  (click)="copyPath(info.path)"
                  class="qt-copy-button qt-copy-button-icon shrink-0"
                  [class.qt-copy-button-success]="copied()"
                  title="Copy path to clipboard"
                  aria-label="Copy path"
                >
                  <qt-icon [name]="copied() ? 'check' : 'copy'" class="w-4 h-4" />
                </button>
              </div>
            </div>

            <!-- Source info -->
            <div>
              <div class="qt-text-label text-sm mb-1">Configuration</div>
              <div class="qt-text-secondary text-sm">{{ info.sourceDescription }}</div>
            </div>

            <!-- Platform info -->
            <div>
              <div class="qt-text-label text-sm mb-1">Platform</div>
              <div class="qt-text-secondary text-sm">{{ platformName(info.platform) }}</div>
            </div>

            <!-- v4 renders a button here when canOpen; v5 has no opener. -->
            <div class="qt-text-muted text-sm qt-bg-muted/30 p-3 rounded">
              <div class="flex items-start gap-2">
                <qt-icon name="external-link" class="w-4 h-4 mt-0.5 shrink-0" />
                @if (info.isDocker) {
                  <div>
                    <p class="font-medium">Docker Environment</p>
                    <p class="mt-1">
                      The data directory is mounted from your host system. Access it through your
                      host's file browser at the path you mounted with
                      <code class="text-xs qt-bg-muted px-1 rounded"
                        >docker run -v /path:/app/quilltap</code
                      >.
                    </p>
                  </div>
                } @else {
                  <div>
                    <p class="font-medium">Opening the folder is not yet at your disposal</p>
                    <p class="mt-1">
                      Quilltap cannot yet summon your file browser on its own account. Copy the path
                      above and open it yourself — a small indignity, and a temporary one.
                    </p>
                  </div>
                }
              </div>
            </div>
          </div>
        } @else {
          <div class="qt-text-error text-sm">Unable to load data directory information</div>
        }
      </div>
    </div>
  `,
})
export class DataDirectorySection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly copied = signal(false);

  protected readonly dirQuery = injectQuery(() => ({
    queryKey: profileKeys.dataDir,
    queryFn: () => fetchDataDir(this.core),
  }));

  protected dirInfo() {
    return this.dirQuery.data() ?? null;
  }

  protected platformName(platform: string): string {
    return PLATFORM_NAMES[platform] ?? platform;
  }

  protected copyPath(path: string): void {
    void navigator.clipboard?.writeText(path).then(() => {
      // v4 keeps BOTH the 2s tick and the toast (`footer-wrapper.tsx:66`).
      this.copied.set(true);
      this.toasts.showSuccess('Path copied to clipboard');
      setTimeout(() => this.copied.set(false), 2000);
    });
  }
}
