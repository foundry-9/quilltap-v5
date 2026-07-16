/**
 * The §3 origin resolver (P4.7b; one-origin reconcile P4.7c): identity in
 * the browser; the `qtap` origin prepended under Tauri when the page lives
 * OFF the qtap origin (`qtap://localhost` on macOS/Linux,
 * `http://qtap.localhost` on Windows — WebView2 cannot register bare custom
 * schemes). On a qtap-origin page (the production shell since P4.7c) the
 * resolver is identity again — that arm is pinned in
 * `api-url.one-origin.spec.ts`, which needs its own jsdom URL. Paths never
 * change; the builder sites are asserted here so a regression at any of
 * them surfaces as a spec diff.
 *
 * These specs run on jsdom's default `http://localhost` page URL, i.e. the
 * `devUrl` dev-loop shape — the prefix arm.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

import { apiUrl } from './api-url';
import { fileUrl, thumbnailUrl } from '../images/image-urls';
import {
  buildMountBlobUrl,
  buildMountFileItemUrl,
} from '../screens/scriptorium/scriptorium.api';

function fakeTauri(): void {
  (globalThis as Record<string, unknown>)['isTauri'] = true;
}

function fakeWindowsUserAgent(): void {
  vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/126.0',
  );
}

afterEach(() => {
  delete (globalThis as Record<string, unknown>)['isTauri'];
  vi.restoreAllMocks();
});

describe('apiUrl', () => {
  it('is identity in the browser', () => {
    expect(apiUrl('/api/v1/files/f-1')).toBe('/api/v1/files/f-1');
    expect(apiUrl('/api/v1/terminals?chatId=c-1')).toBe('/api/v1/terminals?chatId=c-1');
  });

  it('prepends qtap://localhost under Tauri on macOS/Linux', () => {
    fakeTauri();
    expect(apiUrl('/api/v1/files/f-1')).toBe('qtap://localhost/api/v1/files/f-1');
  });

  it('prepends http://qtap.localhost under Tauri on Windows', () => {
    fakeTauri();
    fakeWindowsUserAgent();
    expect(apiUrl('/api/v1/files/f-1')).toBe('http://qtap.localhost/api/v1/files/f-1');
  });

  it('never rewrites the path itself (query and encoded segments intact)', () => {
    fakeTauri();
    expect(apiUrl('/api/v1/mount-points/m%201/files/a%2Fb?action=thumbnail&size=80')).toBe(
      'qtap://localhost/api/v1/mount-points/m%201/files/a%2Fb?action=thumbnail&size=80',
    );
  });
});

describe('the builder sites resolve through apiUrl', () => {
  it('image byte routes (browser: identity)', () => {
    expect(fileUrl('f-1')).toBe('/api/v1/files/f-1');
    expect(thumbnailUrl('f-1')).toBe('/api/v1/files/f-1?action=thumbnail&size=80');
  });

  it('image byte routes (Tauri: qtap origin)', () => {
    fakeTauri();
    expect(fileUrl('f-1')).toBe('qtap://localhost/api/v1/files/f-1');
    expect(thumbnailUrl('f-1', 320)).toBe(
      'qtap://localhost/api/v1/files/f-1?action=thumbnail&size=320',
    );
  });

  it('scriptorium mount routes (browser: identity)', () => {
    expect(buildMountFileItemUrl('m-1', 'docs/a.md')).toBe(
      '/api/v1/mount-points/m-1/files/docs/a.md',
    );
    expect(buildMountBlobUrl('m-1', 'img/p 1.png')).toBe(
      '/api/v1/mount-points/m-1/blobs/img/p%201.png',
    );
  });

  it('scriptorium mount routes (Tauri: qtap origin, per-segment encoding intact)', () => {
    fakeTauri();
    expect(buildMountFileItemUrl('m-1', 'docs/a.md')).toBe(
      'qtap://localhost/api/v1/mount-points/m-1/files/docs/a.md',
    );
    expect(buildMountBlobUrl('m-1', 'img/p 1.png')).toBe(
      'qtap://localhost/api/v1/mount-points/m-1/blobs/img/p%201.png',
    );
  });
});
