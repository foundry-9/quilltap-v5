import { createServer, type Server } from 'node:http';

/**
 * A tiny Serper (`google.serper.dev/search`) mock for the P4.42 web-search beat.
 * The real quilltap-web binary is launched (in global setup) with
 * `SERPER_API_KEY` set and `QUILLTAP_SERPER_BASE_URL` pointed here, so a real
 * `search_web` tool run round-trips through the real binary + spine +
 * `RealWebSearchProvider` over the real blocking HTTP transport — no live Serper
 * call, no spend.
 *
 * It answers any `POST` with a fixed body carrying THREE organic results plus a
 * `knowledgeGraph` with a description, so the response exercises v4's kg-unshift
 * (the kg row is prepended when `results.length < maxResults`).
 */
export interface MockSerper {
  url: string;
  close: () => Promise<void>;
}

/** A distinctive title the beat asserts on (proves the mock's data reached the card). */
export const MOCK_SERPER_KG_TITLE = 'Pharos of Alexandria';

const MOCK_BODY = JSON.stringify({
  organic: [
    {
      title: 'A history of lighthouses',
      link: 'https://example.com/history',
      snippet: 'Beacons that guided ships for centuries.',
      date: '2026-06-15T00:00:00.000Z',
    },
    {
      title: 'Modern lighthouse engineering',
      link: 'https://example.com/engineering',
      snippet: 'How a lamp room is built today.',
    },
    {
      title: 'Famous lighthouse keepers',
      link: 'https://example.com/keepers',
      snippet: 'The lonely work of tending the light.',
    },
  ],
  knowledgeGraph: {
    title: MOCK_SERPER_KG_TITLE,
    description: 'One of the Seven Wonders of the Ancient World.',
    source: { name: 'Wikipedia', link: 'https://example.com/pharos' },
  },
});

export async function startMockSerper(port = 0): Promise<MockSerper> {
  const server = createServer((req, res) => {
    if (req.method !== 'POST') {
      res.writeHead(404).end();
      return;
    }
    // Drain the request body (we ignore it — the reply is fixed).
    req.on('data', () => {});
    req.on('end', () => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(MOCK_BODY);
    });
  });

  const boundPort = await listen(server, port);
  return {
    url: `http://127.0.0.1:${boundPort}/search`,
    close: () =>
      new Promise<void>((resolve, reject) =>
        server.close((err) => (err ? reject(err) : resolve())),
      ),
  };
}

function listen(server: Server, port = 0): Promise<number> {
  return new Promise((resolve) => {
    server.listen(port, '127.0.0.1', () => {
      const addr = server.address();
      resolve(typeof addr === 'object' && addr ? addr.port : 0);
    });
  });
}
