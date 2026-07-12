import { describe, expect, it } from 'vitest';

import type { MemoryDto } from '../core/core-contract';
import {
  appendUniqueMemories,
  importanceColorClass,
  importanceLabel,
  importancePercent,
  keywordsFromString,
  keywordsToString,
} from './memory-format';

function mem(over: Partial<MemoryDto>): MemoryDto {
  return {
    id: 'm1',
    userId: 'u1',
    characterId: 'c1',
    content: 'content',
    summary: 'summary',
    keywords: [],
    tags: [],
    importance: 0.5,
    source: 'MANUAL',
    chatId: null,
    projectId: null,
    aboutCharacterId: null,
    sourceMessageId: null,
    witnessedContext: null,
    lastAccessedAt: null,
    reinforcementCount: 0,
    lastReinforcedAt: null,
    relatedMemoryIds: [],
    reinforcedImportance: 0.5,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

describe('memory-format: importance buckets (v4 memory-card thresholds)', () => {
  it('labels >= 0.7 High, >= 0.4 Medium, else Low (boundaries inclusive)', () => {
    expect(importanceLabel(0.7)).toBe('High');
    expect(importanceLabel(0.69)).toBe('Medium');
    expect(importanceLabel(0.4)).toBe('Medium');
    expect(importanceLabel(0.39)).toBe('Low');
    expect(importanceLabel(0)).toBe('Low');
    expect(importanceLabel(1)).toBe('High');
  });

  it('colors match the label bucket', () => {
    expect(importanceColorClass(0.9)).toBe('qt-text-destructive');
    expect(importanceColorClass(0.5)).toBe('qt-text-warning');
    expect(importanceColorClass(0.1)).toBe('qt-text-secondary');
  });

  it('percent rounds importance*100 (v4 editor label)', () => {
    expect(importancePercent(0.5)).toBe(50);
    expect(importancePercent(0.7)).toBe(70);
    expect(importancePercent(0.155)).toBe(16);
  });
});

describe('memory-format: keywords join/split (v4 memory-editor)', () => {
  it('joins with ", "', () => {
    expect(keywordsToString(['a', 'b', 'c'])).toBe('a, b, c');
    expect(keywordsToString([])).toBe('');
  });

  it('splits on comma, trims, drops empties', () => {
    expect(keywordsFromString('a, b,c ,  , d')).toEqual(['a', 'b', 'c', 'd']);
    expect(keywordsFromString('')).toEqual([]);
    expect(keywordsFromString('   ')).toEqual([]);
    expect(keywordsFromString('solo')).toEqual(['solo']);
  });
});

describe('memory-format: page-dedupe (v4 memory-list append)', () => {
  it('appends only ids not already present', () => {
    const prev = [mem({ id: 'a' }), mem({ id: 'b' })];
    const incoming = [mem({ id: 'b' }), mem({ id: 'c' })];
    const result = appendUniqueMemories(prev, incoming);
    expect(result.map((m) => m.id)).toEqual(['a', 'b', 'c']);
  });

  it('appends the whole page when fully fresh', () => {
    const prev = [mem({ id: 'a' })];
    const incoming = [mem({ id: 'b' }), mem({ id: 'c' })];
    expect(appendUniqueMemories(prev, incoming).map((m) => m.id)).toEqual(['a', 'b', 'c']);
  });

  it('drops a fully-overlapping page (no duplicate keys)', () => {
    const prev = [mem({ id: 'a' }), mem({ id: 'b' })];
    const incoming = [mem({ id: 'a' }), mem({ id: 'b' })];
    expect(appendUniqueMemories(prev, incoming).map((m) => m.id)).toEqual(['a', 'b']);
  });
});
