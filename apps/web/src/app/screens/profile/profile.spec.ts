import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { formatDateTime, formatProfileDate } from '../../shared/format-date';
import { ProfilePage } from './profile-page';
import { fetchProfile, setProfileAvatar, updateProfile } from './profile.api';
import { ToastService } from '../../ui/toast.service';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function stubClient(handler: (req: DispatchReq) => unknown): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      const out = handler(req);
      if (out instanceof Error) {
        throw out;
      }
      return (out ?? {}) as Record<string, unknown>;
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

const PROFILE = {
  id: 'u-1',
  username: 'friday',
  email: 'friday@foundry-9.com',
  name: 'Friday',
  createdAt: '2026-06-01T00:00:00.000Z',
  updatedAt: '2026-06-01T00:00:00.000Z',
};

const DATA_DIR = {
  path: '/tmp/instance',
  source: 'environment',
  sourceDescription: '--data-dir command-line flag (/tmp/instance)',
  platform: 'darwin',
  isDocker: false,
  isVM: false,
  isElectronShell: false,
  shellVersion: null,
  shellCapabilities: [],
  canOpen: true,
  hostPath: '/tmp/instance',
};

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('profile.api (v4 app/api/v1/user/profile/route.ts + ProfileView.tsx)', () => {
  it('tolerates both the {profile} envelope and a bare profile object', async () => {
    // v4 `ProfileView.tsx:24-26` accepts either; v5's server only sends the
    // envelope, but the tolerance is v4's behavior and is carried.
    const wrapped = await fetchProfile(
      stubClient(() => ({ profile: PROFILE })) as CoreClient,
    );
    expect(wrapped.username).toBe('friday');
    const bare = await fetchProfile(stubClient(() => PROFILE) as CoreClient);
    expect(bare.username).toBe('friday');
  });

  it('OMITS an empty field rather than sending null (v4 sends null and gets a 400)', async () => {
    const seen: DispatchReq[] = [];
    const core = stubClient((req) => {
      seen.push(req);
      return { profile: PROFILE };
    }) as CoreClient;

    await updateProfile(core, { name: 'Reginald', email: '' });

    expect(seen[0]?.['type']).toBe('userProfileUpdate');
    expect(seen[0]?.['name']).toBe('Reginald');
    // The claim: absent, NOT null. `api::user_profile` rejects an explicit null
    // exactly as v4 does, so sending one would be a 400 instead of a no-op.
    expect('email' in (seen[0] ?? {})).toBe(false);
  });

  it('sends an explicit null imageId to clear the avatar', async () => {
    const seen: DispatchReq[] = [];
    const core = stubClient((req) => {
      seen.push(req);
      return { profile: PROFILE };
    }) as CoreClient;

    await setProfileAvatar(core, null);

    expect(seen[0]?.['type']).toBe('userProfileSetAvatar');
    // Here null IS the signal — `avatarSchema` requires the key and allows null.
    expect(seen[0]?.['imageId']).toBeNull();
  });
});

describe('format-date (v4 lib/format-time.ts formatDateTime)', () => {
  it("renders '' for a falsy input and 'Never' through the profile wrapper", () => {
    expect(formatDateTime(null)).toBe('');
    expect(formatDateTime(undefined)).toBe('');
    expect(formatDateTime('')).toBe('');
    // v4 `ProfileInfoSection.tsx:12-14`.
    expect(formatProfileDate(null)).toBe('Never');
  });

  it('formats a real timestamp with a long month for the profile rows', () => {
    const out = formatProfileDate('2026-06-01T12:34:00.000Z');
    expect(out).not.toBe('Never');
    expect(out).toMatch(/2026/);
  });
});

describe('ProfilePage (v4 app/profile/ProfileView.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  async function render(
    handler: (req: DispatchReq) => unknown,
  ): Promise<ComponentFixture<ProfilePage>> {
    TestBed.configureTestingModule({
      imports: [ProfilePage],
      providers: [
        { provide: CoreClient, useValue: stubClient(handler) },
        provideRouter([]),
        provideTanStackQuery(
          new QueryClient({ defaultOptions: { queries: { retry: false } } }),
        ),
      ],
    });
    const fixture = TestBed.createComponent(ProfilePage);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('renders the three sections with v4 copy', async () => {
    const fixture = await render((req) =>
      req.type === 'systemDataDir' ? DATA_DIR : { profile: PROFILE },
    );
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';

    expect(text).toContain('Manage your profile settings');
    expect(text).toContain('Profile Settings');
    expect(text).toContain('Account Information');
    expect(text).toContain('Data Directory');
    expect(text).toContain('/tmp/instance');
    // The read-only rows.
    expect(text).toContain('User ID');
    expect(text).toContain('friday');
  });

  it("renders the data-dir open action as v4's non-openable BOX, never a button", async () => {
    // v5 has no opener (the `?action=open` refusal), so even with canOpen true
    // the screen must not offer a button that would fail.
    const fixture = await render((req) =>
      req.type === 'systemDataDir' ? DATA_DIR : { profile: PROFILE },
    );
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).not.toContain('Open in File Browser');
    expect(text).toContain('Opening the folder is not yet at your disposal');
  });

  it("shows v4's Docker copy verbatim when the server really is in Docker", async () => {
    const fixture = await render((req) =>
      req.type === 'systemDataDir'
        ? { ...DATA_DIR, isDocker: true, canOpen: false, platform: 'docker' }
        : { profile: PROFILE },
    );
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Docker Environment');
    expect(text).toContain('docker run -v /path:/app/quilltap');
  });

  it('shows the error state with a way home when the profile fetch fails', async () => {
    const fixture = await render((req) => {
      if (req.type === 'userProfileGet') {
        return new Error('nope');
      }
      return DATA_DIR;
    });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Failed to load profile');
    expect(text).toContain('Return to Home');
  });

  it('sends the edit through and refetches (the save round-trip)', async () => {
    let name = 'Friday';
    const seen: DispatchReq[] = [];
    const fixture = await render((req) => {
      seen.push(req);
      if (req.type === 'systemDataDir') {
        return DATA_DIR;
      }
      if (req.type === 'userProfileUpdate') {
        name = req['name'] as string;
        return { profile: { ...PROFILE, name } };
      }
      return { profile: { ...PROFILE, name } };
    });

    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
      '#profile-name',
    )!;
    input.value = 'Reginald Jeeves';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const save = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button'),
    ).find((b) => b.textContent?.includes('Save Changes'))!;
    expect(save.disabled).toBe(false);
    save.click();
    await settle(fixture);

    expect(seen.some((r) => r.type === 'userProfileUpdate' && r['name'] === 'Reginald Jeeves')).toBe(
      true,
    );
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Profile updated successfully' });
  });

  it('keeps Save disabled until something actually changed (v4 hasChanges)', async () => {
    const fixture = await render((req) =>
      req.type === 'systemDataDir' ? DATA_DIR : { profile: PROFILE },
    );
    const save = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button'),
    ).find((b) => b.textContent?.includes('Save Changes'))!;
    expect(save.disabled).toBe(true);
  });
});
