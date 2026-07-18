import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';

import { apiUrl } from '../../core/api-url';
import { CoreClient } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import { Icon } from '../../ui/icon';
import { AvatarPicker } from './avatar-picker';
import { setProfileAvatar, updateProfile, type UserProfile } from './profile.api';

/**
 * The editable profile block (v4 `components/profile/ProfileEditSection.tsx`):
 * the avatar with its change affordances, Display Name, Email Address, and the
 * Save button that is disabled until something actually changed. Copy verbatim.
 *
 * ## Two v4 details carried on purpose
 *
 * - **The cache-bust key** (`:92-98`): the avatar `src` gets `?v=<n>`, bumped
 *   after every successful set, because the file URL does not change when the
 *   image behind it does. Ported including the `?`-vs-`&` separator choice.
 * - **`hasChanges`** (`:90`) compares against `profile.name || ''`, so a field
 *   that was NULL and is still empty is NOT a change — which is why the Save
 *   button stays disabled on a freshly-loaded blank field.
 *
 * ## One v4 behavior deliberately NOT carried
 *
 * v4 sends `{name: name || null, email: email || null}`, and its own route
 * rejects an explicit null with a 400 (`.optional()` admits absent, not null) —
 * so clearing a box in v4 produces an error and no change. `updateProfile`
 * OMITS an empty field instead, which is the same no-op without the spurious
 * failure. The server keeps v4's rejection verbatim (see the differential's
 * `put_explicit_nulls`); it is the CLIENT that stops walking into it.
 *
 * v5 has no toast system, so v4's `showSuccessToast`/`showErrorToast` become
 * inline status text (the established v5 substitute).
 */
@Component({
  selector: 'qt-profile-edit-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Icon, AvatarPicker],
  template: `
    <div class="qt-card">
      <div class="qt-card-header">
        <h2 class="text-xl font-semibold">Profile Settings</h2>
        <p class="qt-text-muted text-sm mt-1">Manage your personal information</p>
      </div>
      <div class="qt-card-content space-y-6">
        <!-- Avatar -->
        <div class="flex items-center gap-6">
          <div class="relative">
            <div
              class="w-24 h-24 rounded-full overflow-hidden qt-bg-muted flex items-center justify-center"
            >
              @if (avatarSrc(); as src) {
                <img
                  [src]="src"
                  [alt]="profile().name || 'Profile'"
                  class="w-full h-full object-cover"
                />
              } @else {
                <qt-icon name="user" class="w-12 h-12 qt-text-secondary" />
              }
            </div>
            <button
              type="button"
              (click)="pickerOpen.set(true)"
              class="absolute bottom-0 right-0 p-2 rounded-full bg-primary text-primary-foreground qt-shadow-lg transition-colors"
              title="Change avatar"
              aria-label="Change avatar"
            >
              <qt-icon name="pencil" class="w-4 h-4" />
            </button>
          </div>
          <div>
            <p class="font-medium">{{ profile().name || profile().username }}</p>
            <p class="qt-text-muted text-sm">{{ profile().email || 'No email set' }}</p>
            <button
              type="button"
              (click)="pickerOpen.set(true)"
              class="qt-button qt-button-ghost text-sm mt-2"
            >
              Change avatar
            </button>
          </div>
        </div>

        <!-- Name -->
        <div>
          <label for="profile-name" class="qt-label block mb-2">Display Name</label>
          <input
            id="profile-name"
            type="text"
            [(ngModel)]="name"
            placeholder="Enter your display name"
            class="qt-input w-full"
            maxlength="100"
          />
          <p class="qt-text-xs qt-text-secondary mt-1">
            This name is displayed throughout the application
          </p>
        </div>

        <!-- Email -->
        <div>
          <label for="profile-email" class="qt-label block mb-2">Email Address</label>
          <input
            id="profile-email"
            type="email"
            [(ngModel)]="email"
            placeholder="Enter your email address"
            class="qt-input w-full"
          />
          <p class="qt-text-xs qt-text-secondary mt-1">
            Your email may be used for account recovery
          </p>
        </div>

        @if (status(); as message) {
          <p class="qt-text-secondary text-sm" data-testid="profile-status">{{ message }}</p>
        }
        @if (error(); as message) {
          <p class="qt-text-destructive text-sm" data-testid="profile-error">{{ message }}</p>
        }

        <!-- Save -->
        <div class="flex justify-end pt-4 border-t qt-border-default">
          <button
            type="button"
            (click)="save()"
            [disabled]="saving() || !hasChanges()"
            class="qt-button qt-button-primary"
          >
            {{ saving() ? 'Saving...' : 'Save Changes' }}
          </button>
        </div>
      </div>
    </div>

    @if (pickerOpen()) {
      <qt-avatar-picker
        [currentImageId]="currentImageId()"
        (chosen)="setAvatar($event)"
        (cancelled)="pickerOpen.set(false)"
      />
    }
  `,
})
export class ProfileEditSection {
  private readonly core = inject(CoreClient);

  readonly profile = input.required<UserProfile>();
  /** Emitted after any successful write so the page can refetch (v4's `onProfileUpdate`). */
  readonly profileUpdated = output<UserProfile>();

  protected readonly name = signal('');
  protected readonly email = signal('');
  protected readonly saving = signal(false);
  protected readonly status = signal<string | null>(null);
  protected readonly error = signal<string | null>(null);
  protected readonly pickerOpen = signal(false);
  /** v4's `avatarRefreshKey` (`:31`) — the cache-bust counter. */
  protected readonly avatarRefreshKey = signal(0);

  /** v4 `:90` — an empty box over a NULL column is NOT a change. */
  protected readonly hasChanges = computed(() => {
    const p = this.profile();
    return this.name() !== (p.name || '') || this.email() !== (p.email || '');
  });

  constructor() {
    // v4 seeds these from `useState(profile.name || '')` on first render; here
    // the profile arrives async, so re-seed whenever the persisted value moves
    // (the `context-compression-settings` precedent). The only mover is a
    // successful save's refetch, so this resyncs the boxes to what was stored
    // rather than fighting the typist.
    effect(() => {
      const p = this.profile();
      this.name.set(p.name || '');
      this.email.set(p.email || '');
    });
  }

  /**
   * v4 `:93-98` — the cache-busted avatar src. The separator choice mirrors
   * v4's (`?` unless the stored URL already has a query string).
   */
  protected avatarSrc(): string | null {
    const image = this.profile().image;
    if (!image) {
      return null;
    }
    const separator = image.includes('?') ? '&' : '?';
    return `${apiUrl(image)}${separator}v=${this.avatarRefreshKey()}`;
  }

  /** v4 `:206` — the stored value is a URL, so the file id is its last segment. */
  protected currentImageId(): string | null {
    const image = this.profile().image;
    return image ? (image.split('/').pop()?.split('?')[0] ?? null) : null;
  }

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.status.set(null);
    this.error.set(null);
    try {
      const updated = await updateProfile(this.core, {
        name: this.name(),
        email: this.email(),
      });
      this.profileUpdated.emit(updated);
      this.status.set('Profile updated successfully');
    } catch (err) {
      this.error.set(this.message(err, 'Failed to update profile'));
    } finally {
      this.saving.set(false);
    }
  }

  protected async setAvatar(imageId: string | null): Promise<void> {
    this.status.set(null);
    this.error.set(null);
    try {
      const updated = await setProfileAvatar(this.core, imageId);
      this.profileUpdated.emit(updated);
      this.avatarRefreshKey.update((k) => k + 1);
      this.pickerOpen.set(false);
      this.status.set('Avatar updated successfully');
    } catch (err) {
      this.error.set(this.message(err, 'Failed to update avatar'));
    }
  }

  private message(err: unknown, fallback: string): string {
    if (err instanceof CoreDispatchError || err instanceof Error) {
      return err.message || fallback;
    }
    return fallback;
  }
}
