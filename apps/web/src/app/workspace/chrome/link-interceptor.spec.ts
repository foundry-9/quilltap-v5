/**
 * interpretWorkspaceLinkClick — the capture-phase link decision (v4
 * `WorkspaceLinkInterceptor.tsx`). Uses jsdom anchors.
 *
 * @vitest-environment jsdom
 */

import { describe, expect, it } from 'vitest';

import { interpretWorkspaceLinkClick } from './link-interceptor';

function anchor(attrs: Record<string, string>): HTMLAnchorElement {
  const a = document.createElement('a');
  for (const [k, v] of Object.entries(attrs)) a.setAttribute(k, v);
  return a;
}

function click(target: HTMLElement | null, over: Partial<MouseEvent> = {}): MouseEvent {
  return {
    button: 0,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    defaultPrevented: false,
    target,
    ...over,
  } as unknown as MouseEvent;
}

describe('interpretWorkspaceLinkClick', () => {
  it('maps an in-app anchor href to a tab intent', () => {
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/characters' })))).toEqual({
      kind: 'aurora',
    });
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon/abc' })))).toEqual({
      kind: 'salon',
      payload: { chatId: 'abc' },
    });
  });

  it('resolves the nearest ancestor anchor from a nested target', () => {
    const a = anchor({ href: '/prospero' });
    const span = document.createElement('span');
    a.appendChild(span);
    expect(interpretWorkspaceLinkClick(click(span))).toEqual({ kind: 'prospero' });
  });

  it('passes through /salon/new (v5 has no NewChatModal — the page navigates)', () => {
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon/new' })))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon/new?projectId=p' })))).toBeNull();
  });

  it('passes through modifier / middle clicks', () => {
    const a = anchor({ href: '/characters' });
    expect(interpretWorkspaceLinkClick(click(a, { metaKey: true }))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(a, { ctrlKey: true }))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(a, { shiftKey: true }))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(a, { button: 1 }))).toBeNull();
  });

  it('passes through download / target / external / already-prevented', () => {
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/characters', download: '' })))).toBeNull();
    expect(
      interpretWorkspaceLinkClick(click(anchor({ href: '/characters', target: '_blank' }))),
    ).toBeNull();
    expect(interpretWorkspaceLinkClick(click(anchor({ href: 'https://x.example' })))).toBeNull();
    expect(
      interpretWorkspaceLinkClick(click(anchor({ href: '/characters' }), { defaultPrevented: true })),
    ).toBeNull();
  });

  it('passes through when there is no anchor, and for hrefs with no tab equivalent', () => {
    expect(interpretWorkspaceLinkClick(click(document.createElement('div')))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/unlock' })))).toBeNull();
  });

  // v4 `8d86847a`: the salon list is a tab now — the rail's Chats item and the
  // home links stop unmounting the workspace.
  it('opens the salon-list tab for /salon (and legacy /chats)', () => {
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon' })))).toEqual({
      kind: 'salon-list',
    });
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/chats' })))).toEqual({
      kind: 'salon-list',
    });
  });
});
