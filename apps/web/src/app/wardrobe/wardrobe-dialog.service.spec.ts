import { describe, expect, it } from 'vitest';

import { WardrobeDialogService } from './wardrobe-dialog.service';

describe('WardrobeDialogService (v4 wardrobe-dialog-provider.tsx)', () => {
  it('starts closed with no context', () => {
    const svc = new WardrobeDialogService();
    expect(svc.isOpen()).toBe(false);
    expect(svc.context()).toBeNull();
  });

  it('open() stores the context and opens; open() with nothing clears it (v4 :43-46)', () => {
    const svc = new WardrobeDialogService();
    svc.open({ characterId: 'c1', chatId: 'ch1' });
    expect(svc.isOpen()).toBe(true);
    expect(svc.context()).toEqual({ characterId: 'c1', chatId: 'ch1' });
    svc.open();
    expect(svc.context()).toBeNull();
  });

  it('close() only flips isOpen, keeping the last context (v4 :48-50)', () => {
    const svc = new WardrobeDialogService();
    svc.open({ characterId: 'c1' });
    svc.close();
    expect(svc.isOpen()).toBe(false);
    expect(svc.context()).toEqual({ characterId: 'c1' });
  });

  it('remountKey is `${characterId}|${chatId}` with auto/no-chat fallbacks (v4 wardrobe-control-dialog.tsx:92)', () => {
    const svc = new WardrobeDialogService();
    expect(svc.remountKey()).toBe('auto|no-chat');
    svc.open({ characterId: 'c1' });
    expect(svc.remountKey()).toBe('c1|no-chat');
    svc.open({ characterId: 'c1', chatId: 'ch1' });
    expect(svc.remountKey()).toBe('c1|ch1');
    svc.open({ chatId: 'ch1' });
    expect(svc.remountKey()).toBe('auto|ch1');
  });
});
