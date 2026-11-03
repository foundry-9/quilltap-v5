// Transcribed case for case from v4 `__tests__/unit/app/salon/components/message-row-diagram-style.test.ts` (236c04e35); v5-authored cases are prefixed `v5:`.
/**
 * Whose hand a Salon message's diagrams are drawn in
 * (resolveMessageDiagramStyle, app/salon/[id]/components/message-row/helpers.ts).
 */

import { describe, expect, it } from 'vitest';

import { resolveMessageDiagramStyle } from './message-diagram-style'

const participants = [
  { id: 'seat-riya', character: { id: 'c-riya', name: 'Riya', talkativeness: 0.5, diagramStyle: 'clean' as const } },
  { id: 'seat-lorian', character: { id: 'c-lorian', name: 'Lorian', talkativeness: 0.5 } },
  { id: 'seat-empty', character: null },
]

describe('resolveMessageDiagramStyle', () => {
  it("uses the speaking character's style", () => {
    expect(resolveMessageDiagramStyle({ participantId: 'seat-riya' }, participants)).toBe('clean')
  })

  it('reads a character with no style on record as hand-drawn', () => {
    expect(resolveMessageDiagramStyle({ participantId: 'seat-lorian' }, participants)).toBe('handDrawn')
  })

  it('draws the operator speaking as no one in the clean style', () => {
    expect(resolveMessageDiagramStyle({ participantId: null }, participants)).toBe('clean')
    expect(resolveMessageDiagramStyle({}, participants)).toBe('clean')
  })

  it('draws every Staff sender in the clean style, whatever the seat', () => {
    expect(resolveMessageDiagramStyle({ participantId: 'seat-lorian', systemSender: 'carina' }, participants)).toBe('clean')
    expect(resolveMessageDiagramStyle({ systemSender: 'host' }, participants)).toBe('clean')
  })

  it('falls back to clean for an unknown or character-less seat', () => {
    expect(resolveMessageDiagramStyle({ participantId: 'seat-gone' }, participants)).toBe('clean')
    expect(resolveMessageDiagramStyle({ participantId: 'seat-empty' }, participants)).toBe('clean')
  })
})
