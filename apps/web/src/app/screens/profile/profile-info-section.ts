import { ChangeDetectionStrategy, Component, input, signal } from '@angular/core';

import { Icon } from '../../ui/icon';
import { formatProfileDate } from '../../shared/format-date';
import type { UserProfile } from './profile.api';

/**
 * One read-only info row with an optional copy button (v4
 * `ProfileInfoSection.tsx:19-53`'s `InfoField`). The copy state is per-row, so
 * it lives in the row component exactly as v4's `useCopyToClipboard` hook does.
 */
@Component({
  selector: 'qt-profile-info-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="flex items-center justify-between py-3 border-b qt-border-default last:border-b-0">
      <div>
        <dt class="qt-text-label text-sm">{{ label() }}</dt>
        <dd class="qt-text-primary font-mono text-sm mt-1">{{ value() }}</dd>
      </div>
      @if (copyable()) {
        <button
          type="button"
          (click)="copy()"
          class="qt-copy-button qt-copy-button-icon ml-2"
          [class.qt-copy-button-success]="copied()"
          title="Copy to clipboard"
          [attr.aria-label]="'Copy ' + label()"
        >
          <qt-icon [name]="copied() ? 'check' : 'copy'" class="w-4 h-4" />
        </button>
      }
    </div>
  `,
})
export class ProfileInfoField {
  readonly label = input.required<string>();
  readonly value = input.required<string>();
  readonly copyable = input(false);

  protected readonly copied = signal(false);

  protected copy(): void {
    void navigator.clipboard?.writeText(this.value()).then(() => {
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 1500);
    });
  }
}

/**
 * The read-only account block (v4 `components/profile/ProfileInfoSection.tsx`):
 * User ID (copyable) / Username / Account Created / Last Updated. Copy verbatim.
 *
 * v4's docstring lists an "Email Verified date" the component never renders
 * (`:75-80` has four fields, not five) — the rendered set is what ports.
 */
@Component({
  selector: 'qt-profile-info-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ProfileInfoField],
  template: `
    <div class="qt-card">
      <div class="qt-card-header">
        <h2 class="text-xl font-semibold">Account Information</h2>
        <p class="qt-text-muted text-sm mt-1">System information about your account</p>
      </div>
      <div class="qt-card-content">
        <dl>
          <qt-profile-info-field label="User ID" [value]="profile().id" [copyable]="true" />
          <qt-profile-info-field label="Username" [value]="profile().username" />
          <qt-profile-info-field label="Account Created" [value]="created()" />
          <qt-profile-info-field label="Last Updated" [value]="updated()" />
        </dl>
      </div>
    </div>
  `,
})
export class ProfileInfoSection {
  readonly profile = input.required<UserProfile>();

  protected created(): string {
    return formatProfileDate(this.profile().createdAt);
  }

  protected updated(): string {
    return formatProfileDate(this.profile().updatedAt);
  }
}
