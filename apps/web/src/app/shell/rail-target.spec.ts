import { describe, expect, it } from 'vitest';

import { railTarget } from './rail-target';

/** The rail Inventory entry's three arms (v4 `sidebar-footer.tsx:255-279`, P4.D330). */
describe('railTarget', () => {
  it('the workspace opens the tab (query string ignored) — only with tabs enabled', () => {
    expect(railTarget('/workspace', true)).toEqual({ kind: 'tab' });
    expect(railTarget('/workspace?open=photos', true)).toEqual({ kind: 'tab' });
    expect(railTarget('/workspace', false)).toEqual({ kind: 'dialog' });
  });

  it('a Salon chat path scopes the dialog to that chat', () => {
    expect(railTarget('/salon/0b6c5c3e-1111-4222-8333-444455556666', true)).toEqual({ kind: 'chat', chatId: '0b6c5c3e-1111-4222-8333-444455556666' });
  });

  it('anywhere else opens the plain dialog', () => {
    expect(railTarget('/settings', true)).toEqual({ kind: 'dialog' });
    expect(railTarget('/salon', true)).toEqual({ kind: 'dialog' });
  });
});
