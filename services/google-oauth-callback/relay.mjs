// Stateless browser redirect. Tokens and OAuth client credentials stay on the Pi.
const callbackPath = '/oauth/google/callback';
const returnPath = '/api/google/callback';
const headers = {
  'Cache-Control': 'no-store',
  'Pragma': 'no-cache',
  'Referrer-Policy': 'no-referrer',
  'Content-Security-Policy': "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
  'X-Content-Type-Options': 'nosniff',
};

export function localReturn(value) {
  if (typeof value !== 'string' || value.length > 2048) throw new Error('Invalid return address');
  const url = new URL(value);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash || url.pathname !== returnPath) throw new Error('Invalid return address');
  const host = url.hostname.toLowerCase();
  const match = /^(\d+)\.(\d+)\.(\d+)\.(\d+)$/.exec(host);
  const privateV4 = match && match.slice(1).every(n => Number(n) <= 255) && (
    Number(match[1]) === 10 || Number(match[1]) === 127 ||
    (Number(match[1]) === 192 && Number(match[2]) === 168) ||
    (Number(match[1]) === 172 && Number(match[2]) >= 16 && Number(match[2]) <= 31)
  );
  const privateV6 = host === '[::1]' || /^\[f[cd][0-9a-f]{2}:/.test(host);
  const localName = host === 'localhost' || /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(host) || /^[a-z0-9.-]+\.local$/.test(host);
  if (!privateV4 && !privateV6 && !localName) throw new Error('Return address must be on the local network');
  return url;
}

export function handle(request) {
  const url = new URL(request.url);
  if (request.method !== 'GET') return new Response('Method not allowed', {status: 405, headers: {...headers, Allow: 'GET'}});
  if (url.pathname === '/healthz') return new Response('ok\n', {headers});
  if (url.pathname === '/') return new Response('Paperphase Google authorisation callback. Start account linking from your Paperphase editor.\n', {headers});
  if (url.pathname !== callbackPath) return new Response('Not found', {status: 404, headers});
  try {
    if (url.href.length > 8192) throw new Error('Response too large');
    for (const name of ['state', 'code', 'error']) if (url.searchParams.getAll(name).length > 1) throw new Error('Duplicate parameter');
    const state = url.searchParams.get('state');
    if (!state || state.length > 4096 || !/^[A-Za-z0-9_-]+$/.test(state)) throw new Error('Invalid state');
    const payload = JSON.parse(Buffer.from(state, 'base64url').toString('utf8'));
    if (!payload || Array.isArray(payload) || !/^[A-Za-z0-9_-]{43}$/.test(payload.nonce)) throw new Error('Invalid state');
    const target = localReturn(payload.return_to);
    const code = url.searchParams.get('code');
    const error = url.searchParams.get('error');
    if ((code === null) === (error === null)) throw new Error('Missing or ambiguous result');
    if (code !== null && (code.length === 0 || code.length > 4096 || /[\x00-\x1f\x7f]/.test(code))) throw new Error('Invalid code');
    if (error !== null && !/^[a-z_]{1,128}$/.test(error)) throw new Error('Invalid error');
    target.searchParams.set('state', state);
    target.searchParams.set(code === null ? 'error' : 'code', code ?? error);
    return new Response(null, {status: 303, headers: {...headers, Location: target.href}});
  } catch {
    // Never reflect the response, code or state in an error page or a log.
    return new Response('Unable to return to Paperphase. Open your editor on the same local network and start account linking again.\n', {status: 400, headers});
  }
}
