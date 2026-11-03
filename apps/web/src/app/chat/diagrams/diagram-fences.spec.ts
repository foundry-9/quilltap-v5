// Transcribed case for case from v4 `__tests__/unit/lib/markdown/diagrams.test.ts` (236c04e35); v5-authored cases are prefixed `v5:`.
/**
 * Unit tests for the shared diagram helpers (lib/markdown/diagrams.ts):
 * which fence languages are diagrams, the lenient `diagramStyle` read, and
 * `hasDiagramFence` — the check that keeps a diagram message off the
 * server-pre-rendered fast path.
 */

import { describe, expect, it } from 'vitest';

import {
  DEFAULT_DIAGRAM_STYLE,
  diagramEngineFor,
  diagramStyleOrDefault,
  hasDiagramFence,
  listClosedDiagramFences,
} from './diagram-fences'

describe('diagramEngineFor', () => {
  it('maps each diagram language to its engine', () => {
    expect(diagramEngineFor('mermaid')).toBe('mermaid')
    expect(diagramEngineFor('dot')).toBe('graphviz')
    expect(diagramEngineFor('graphviz')).toBe('graphviz')
    expect(diagramEngineFor('svg')).toBe('svg')
  })

  it('ignores case and surrounding whitespace', () => {
    expect(diagramEngineFor(' Mermaid ')).toBe('mermaid')
    expect(diagramEngineFor('DOT')).toBe('graphviz')
  })

  it('returns null for ordinary code languages and empty input', () => {
    expect(diagramEngineFor('typescript')).toBeNull()
    expect(diagramEngineFor('text')).toBeNull()
    expect(diagramEngineFor('')).toBeNull()
    expect(diagramEngineFor(null)).toBeNull()
    expect(diagramEngineFor(undefined)).toBeNull()
  })

  it('does not treat Object.prototype keys as languages', () => {
    expect(diagramEngineFor('constructor')).toBeNull()
    expect(diagramEngineFor('toString')).toBeNull()
  })
})

describe('diagramStyleOrDefault', () => {
  it('keeps the two known styles', () => {
    expect(diagramStyleOrDefault('handDrawn')).toBe('handDrawn')
    expect(diagramStyleOrDefault('clean')).toBe('clean')
  })

  it('reads absent and unrecognized values as hand-drawn', () => {
    expect(DEFAULT_DIAGRAM_STYLE).toBe('handDrawn')
    expect(diagramStyleOrDefault(undefined)).toBe('handDrawn')
    expect(diagramStyleOrDefault(null)).toBe('handDrawn')
    expect(diagramStyleOrDefault('sketchy')).toBe('handDrawn')
    expect(diagramStyleOrDefault(42)).toBe('handDrawn')
  })
})

describe('hasDiagramFence', () => {
  it('finds a closed backtick fence', () => {
    expect(hasDiagramFence('Here:\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nDone.')).toBe(true)
  })

  it('finds closed dot, graphviz and svg fences', () => {
    expect(hasDiagramFence('```dot\ndigraph { a -> b }\n```')).toBe(true)
    expect(hasDiagramFence('```graphviz\ngraph { a -- b }\n```')).toBe(true)
    expect(hasDiagramFence('```svg\n<svg></svg>\n```')).toBe(true)
  })

  it('finds a closed tilde fence', () => {
    expect(hasDiagramFence('~~~mermaid\nflowchart TD\n  A --> B\n~~~')).toBe(true)
  })

  it('accepts a longer closing fence and an info string after the language', () => {
    expect(hasDiagramFence('````mermaid title="x"\nflowchart TD\n````')).toBe(true)
    expect(hasDiagramFence('```mermaid\nflowchart TD\n`````')).toBe(true)
  })

  it('finds a fence inside a blockquote', () => {
    expect(hasDiagramFence('> ```mermaid\n> flowchart TD\n> ```')).toBe(true)
  })

  it('ignores an unterminated fence (a message still streaming)', () => {
    expect(hasDiagramFence('Sketching…\n\n```mermaid\nflowchart TD\n  A --> B')).toBe(false)
  })

  it('does not let a shorter or different fence close it', () => {
    expect(hasDiagramFence('````mermaid\nflowchart TD\n```')).toBe(false)
    expect(hasDiagramFence('```mermaid\nflowchart TD\n~~~')).toBe(false)
  })

  it('ignores ordinary code fences', () => {
    expect(hasDiagramFence('```ts\nconst x = 1\n```')).toBe(false)
    expect(hasDiagramFence('```\nplain\n```')).toBe(false)
  })

  it('ignores a diagram fence quoted inside another fence', () => {
    const quoted = '````markdown\n```mermaid\nflowchart TD\n```\n````'
    expect(hasDiagramFence(quoted)).toBe(false)
  })

  it('ignores mermaid mentioned in prose or an inline code span', () => {
    expect(hasDiagramFence('I could draw it in mermaid if you like.')).toBe(false)
    expect(hasDiagramFence('Use `mermaid` or ```mermaid``` inline.')).toBe(false)
  })

  it('finds a diagram fence that follows an ordinary one', () => {
    expect(hasDiagramFence('```ts\nx\n```\n\n```svg\n<svg/>\n```')).toBe(true)
  })

  it('handles empty input', () => {
    expect(hasDiagramFence('')).toBe(false)
  })
})

describe('listClosedDiagramFences', () => {
  it('v5: lists two closed fences in document order, with their sources', () => {
    expect(
      listClosedDiagramFences('```mermaid\ngraph TD; A-->B\n```\n\ntext\n\n```dot\ndigraph { a -> b }\n```'),
    ).toEqual([
      { engine: 'mermaid', language: 'mermaid', source: 'graph TD; A-->B' },
      { engine: 'graphviz', language: 'dot', source: 'digraph { a -> b }' },
    ])
  })

  it('v5: keeps a multi-line source without its trailing newline', () => {
    expect(listClosedDiagramFences('```mermaid\nflowchart TD\n  A --> B\n```')[0].source).toBe(
      'flowchart TD\n  A --> B',
    )
  })

  it('v5: counts a diagram fence nested inside a ~~~~ fence as text', () => {
    expect(listClosedDiagramFences('~~~~markdown\n```mermaid\nflowchart TD\n```\n~~~~')).toEqual([])
  })

  it('v5: excludes an unterminated fence but keeps the closed one before it', () => {
    const fences = listClosedDiagramFences('```svg\n<svg/>\n```\n\n```mermaid\nflowchart TD')
    expect(fences.map((f) => f.engine)).toEqual(['svg'])
  })

  it('v5: finds a capitalised fence and keeps its language as written', () => {
    expect(listClosedDiagramFences('```Mermaid\nflowchart TD\n```')).toEqual([
      { engine: 'mermaid', language: 'Mermaid', source: 'flowchart TD' },
    ])
  })

  it('v5: skips a backtick fence whose info string carries a backtick', () => {
    expect(listClosedDiagramFences('```mermaid `x`\nflowchart TD\n```')).toEqual([])
  })

  it("v5: strips a blockquote's markers from the source, as remark does", () => {
    expect(listClosedDiagramFences('> ```mermaid\n> flowchart TD\n>   A --> B\n> ```')).toEqual([
      { engine: 'mermaid', language: 'mermaid', source: 'flowchart TD\n  A --> B' },
    ])
  })

  it('v5: removes up to the opening fence indentation from each line', () => {
    expect(listClosedDiagramFences('  ```mermaid\n  flowchart TD\n    A --> B\n  ```')[0].source).toBe(
      'flowchart TD\n  A --> B',
    )
  })
})
