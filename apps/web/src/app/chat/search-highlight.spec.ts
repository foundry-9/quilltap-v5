import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CONVERSATION_SEARCH_HIGHLIGHT, SearchHighlight } from './search-highlight';

/**
 * v4 `useSearchHighlight` (`4f5047241`) as `SearchHighlight`. jsdom has no
 * Custom Highlight API, so `CSS.highlights` and `Highlight` are stubbed with a
 * Map and a Set-backed class — enough to watch which ranges each instance adds
 * and deletes.
 */

class FakeHighlight {
  readonly ranges = new Set<Range>();
  add(range: Range): this {
    this.ranges.add(range);
    return this;
  }
  delete(range: Range): boolean {
    return this.ranges.delete(range);
  }
}

let registry: Map<string, FakeHighlight>;
let frames: Array<FrameRequestCallback | null>;

function shared(): FakeHighlight {
  const h = registry.get(CONVERSATION_SEARCH_HIGHLIGHT);
  if (!h) throw new Error('no shared highlight registered');
  return h;
}

/** The painted text of every range in the shared highlight, in insertion order. */
function painted(): string[] {
  return [...shared().ranges].map((r) => r.toString());
}

function makeContainer(html: string): HTMLElement {
  const el = document.createElement('div');
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

/** Let MutationObserver callbacks (microtasks) run. */
const flushObservers = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

function runFrames(): void {
  const pending = frames;
  frames = [];
  for (const cb of pending) cb?.(0);
}

describe('SearchHighlight', () => {
  beforeEach(() => {
    registry = new Map();
    frames = [];
    vi.stubGlobal('CSS', { highlights: registry });
    vi.stubGlobal('Highlight', FakeHighlight);
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      frames.push(cb);
      return frames.length;
    });
    vi.stubGlobal('cancelAnimationFrame', (id: number) => {
      frames[id - 1] = null;
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.body.innerHTML = '';
  });

  it('paints every occurrence case-insensitively and removes its own ranges on dispose', () => {
    const c = makeContainer('<p>A fine day, a FINE day.</p><p>fine again</p>');
    const h = new SearchHighlight();
    h.attach(c, 'fine');
    expect(painted()).toEqual(['fine', 'FINE', 'fine']);
    h.dispose();
    expect(painted()).toEqual([]);
  });

  it('creates ONE shared Highlight under the conversation-search name', () => {
    const a = new SearchHighlight();
    const b = new SearchHighlight();
    a.attach(makeContainer('<p>one fine</p>'), 'fine');
    const first = shared();
    b.attach(makeContainer('<p>two fine</p>'), 'fine');
    expect(registry.size).toBe(1);
    expect(shared()).toBe(first);
  });

  it("two instances never delete each other's ranges", () => {
    const a = new SearchHighlight();
    const b = new SearchHighlight();
    a.attach(makeContainer('<p>alpha club</p>'), 'club');
    b.attach(makeContainer('<p>beta club and club</p>'), 'club');
    expect(painted()).toHaveLength(3);
    a.dispose();
    expect(painted()).toEqual(['club', 'club']);
    expect([...shared().ranges].every((r) => r.startContainer.textContent === 'beta club and club')).toBe(
      true,
    );
    // Re-attaching b with a new query replaces only b's own paint.
    a.attach(makeContainer('<p>gamma beta</p>'), 'gamma');
    b.attach(document.body.children[1] as HTMLElement, 'beta');
    expect(painted().sort()).toEqual(['beta', 'gamma']);
  });

  it('skips the composer, inline editors, scripts and styles', () => {
    const c = makeContainer(
      [
        '<p>shown word</p>',
        '<input value="word">',
        '<textarea>word</textarea>',
        '<div contenteditable="true"><span>word</span></div>',
        '<div contenteditable="false">plain word</div>',
        '<script>var word = 1</script>',
        '<style>.word{}</style>',
      ].join(''),
    );
    const h = new SearchHighlight();
    h.attach(c, 'word');
    expect([...shared().ranges].map((r) => r.startContainer.textContent)).toEqual([
      'shown word',
      'plain word',
    ]);
  });

  it('a null or one-character query paints nothing (and clears earlier paint)', () => {
    const c = makeContainer('<p>a fine a</p>');
    const h = new SearchHighlight();
    h.attach(c, 'fine');
    expect(painted()).toEqual(['fine']);
    h.attach(c, null);
    expect(painted()).toEqual([]);
    h.attach(c, ' a ');
    expect(painted()).toEqual([]);
  });

  it('repaints after a mutation, coalesced to one frame', async () => {
    const c = makeContainer('<p>one fine</p>');
    const h = new SearchHighlight();
    h.attach(c, 'fine');
    expect(painted()).toEqual(['fine']);

    const p = document.createElement('p');
    p.textContent = 'two fine';
    c.appendChild(p);
    await flushObservers();
    (c.firstChild as HTMLElement).firstChild!.nodeValue = 'one fine fine';
    await flushObservers();
    const q = document.createElement('p');
    q.textContent = 'three fine';
    c.appendChild(q);
    await flushObservers();

    // Three mutation batches, ONE frame requested; nothing repainted until it
    // runs (the one old range is still the only one — rewriting its text node
    // collapsed it, which is exactly why the observer must repaint).
    expect(frames).toHaveLength(1);
    expect(shared().ranges.size).toBe(1);
    runFrames();
    expect(painted()).toHaveLength(4);

    // After the frame ran, the next mutation may schedule one more.
    q.remove();
    await flushObservers();
    expect(frames).toHaveLength(1);
    runFrames();
    expect(painted()).toHaveLength(3);
  });

  it('dispose disconnects the observer and cancels a pending frame', async () => {
    const c = makeContainer('<p>one fine</p>');
    const h = new SearchHighlight();
    h.attach(c, 'fine');
    c.appendChild(document.createElement('p'));
    await flushObservers();
    expect(frames).toHaveLength(1);
    h.dispose();
    expect(frames[0]).toBeNull();
    c.appendChild(document.createElement('p'));
    await flushObservers();
    expect(frames).toHaveLength(1);
    expect(painted()).toEqual([]);
  });

  it('is a no-op without the Custom Highlight API', () => {
    vi.stubGlobal('CSS', {});
    const observe = vi.spyOn(MutationObserver.prototype, 'observe');
    const h = new SearchHighlight();
    expect(() => h.attach(makeContainer('<p>fine</p>'), 'fine')).not.toThrow();
    expect(observe).not.toHaveBeenCalled();
    expect(registry.size).toBe(0);
    h.dispose();
    observe.mockRestore();
  });
});
