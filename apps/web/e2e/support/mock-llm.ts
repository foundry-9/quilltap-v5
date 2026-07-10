import { createServer, type Server } from 'node:http';

/**
 * A tiny OPENAI-compatible chat-completions mock for the M4 e2e. It speaks the
 * streaming SSE the provider transport expects (`POST /v1/chat/completions` with
 * `stream: true`), emitting a fixed reply as a few `content` deltas then
 * `[DONE]`. The M4 global setup rewrites the Salon fixture's OPENAI_COMPATIBLE
 * connection profile `baseUrl` to this server, so a real browser send round-trips
 * through the real binary + spine without any live LLM.
 */
export interface MockLlm {
  url: string;
  close: () => Promise<void>;
}

export const MOCK_LLM_REPLY = 'The kettle is on. Do come in.';

/** The model ids the mock advertises (the Settings wizard/profile-modal fetch). */
export const MOCK_LLM_MODELS = ['mock-model', 'mock-model-mini'];

export async function startMockLlm(reply: string = MOCK_LLM_REPLY, port = 0): Promise<MockLlm> {
  const server = createServer((req, res) => {
    // The models-list + validate probe (OPENAI_COMPATIBLE `GET {baseUrl}/models`
    // → `{ data: [{ id }] }`; a successful list is also the connection validate).
    if (
      req.method === 'GET' &&
      req.url?.includes('/models') &&
      !req.url.includes('/chat/completions')
    ) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: MOCK_LLM_MODELS.map((id) => ({ id })) }));
      return;
    }
    if (req.method !== 'POST' || !req.url?.includes('/chat/completions')) {
      res.writeHead(404).end();
      return;
    }
    // Drain the request body (we ignore it — the reply is fixed).
    req.on('data', () => {});
    req.on('end', () => {
      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
      });
      const words = reply.split(' ');
      const model = 'mock-model';
      for (let i = 0; i < words.length; i++) {
        const content = (i === 0 ? '' : ' ') + words[i];
        const chunk = {
          id: 'mock-1',
          object: 'chat.completion.chunk',
          model,
          choices: [{ index: 0, delta: { content }, finish_reason: null }],
        };
        res.write(`data: ${JSON.stringify(chunk)}\n\n`);
      }
      const finalChunk = {
        id: 'mock-1',
        object: 'chat.completion.chunk',
        model,
        choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
        usage: {
          prompt_tokens: 10,
          completion_tokens: words.length,
          total_tokens: 10 + words.length,
        },
      };
      res.write(`data: ${JSON.stringify(finalChunk)}\n\n`);
      res.write('data: [DONE]\n\n');
      res.end();
    });
  });

  const boundPort = await listen(server, port);
  return {
    url: `http://127.0.0.1:${boundPort}`,
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

// Standalone runner: `node mock-llm.js [reply]` (used if the spec spawns it as a
// separate process instead of in-process).
if (require.main === module) {
  void startMockLlm(process.argv[2]).then((m) => {
    process.stdout.write(`${m.url}\n`);
  });
}
