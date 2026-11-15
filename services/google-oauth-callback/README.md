# Paperphase Google callback

A stateless browser redirect for headless Pis. It never contacts a Pi, exchanges
a Google code, or stores OAuth credentials. Each Pi supplies a one-use login
state with its current editor address; the browser returns through its own LAN.

Deployment and Google Cloud setup:
[Google Calendar operator guide](../../docs/usage/google-calendar.md).
Protocol and multiple-Pi isolation:
[Google Calendar design](../../docs/design/google-calendar.md).

```sh
node --test relay.test.mjs
node server.mjs
```

Node 24+; no external packages. Production: build the supplied Dockerfile, join
Caddy's existing Docker network with Compose, and route
`auth.paperphase.oddorbit.software` to `paperphase-auth:8080`. See
`Caddyfile.example`. Do not enable request URL logging for the callback.
