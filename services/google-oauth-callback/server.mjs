import {createServer} from 'node:http';
import {handle} from './relay.mjs';

const port = Number(process.env.PORT || 8080);
const server = createServer(async (incoming, outgoing) => {
  try {
    // The public host is not used for return routing; Caddy owns public TLS.
    const request = new Request(new URL(incoming.url, 'http://callback.internal'), {method: incoming.method});
    const response = handle(request);
    outgoing.writeHead(response.status, Object.fromEntries(response.headers));
    outgoing.end(await response.text());
  } catch {
    outgoing.writeHead(400, {'Cache-Control': 'no-store', 'Content-Type': 'text/plain'});
    outgoing.end('Invalid request\n');
  }
});
server.headersTimeout = 10000;
server.requestTimeout = 15000;
server.maxHeadersCount = 32;
server.listen(port, '0.0.0.0', () => console.log(`Paperphase callback listening on port ${port}`));
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => server.close(() => process.exit(0)));
