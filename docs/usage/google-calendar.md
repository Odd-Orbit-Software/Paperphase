# Google Calendar setup

The hosted callback container belongs on the owner's VPS behind Caddy. The owner
deploys it themselves; Paperphase does not require access to that VPS. Each Pi
keeps its own Google credentials and linked accounts. The browser returns to the
actual editor address, so its hostname does not have to be `paperphase`.

## Deploy the callback

Copy `services/google-oauth-callback` to the VPS. Point DNS for
`auth.paperphase.oddorbit.software` at that VPS (only publish AAAA if IPv6 reaches
Caddy). The supplied Compose file assumes Caddy also runs in Docker and is on an
existing external network named `caddy`. Set `CADDY_NETWORK` to its real name.

From the copied directory:

```sh
CADDY_NETWORK=caddy docker compose up -d --build
```

Add the site's block from `Caddyfile.example` to the existing Caddy configuration
and validate/reload it using the server's usual procedure. Caddy resolves
`paperphase-auth:8080` over the shared Docker network; do not publish the
container port publicly. The example disables this site's access logs because
callback query strings contain authorisation codes. Ensure any upstream proxy
also avoids retaining callback URLs, and avoid verbose HTTP debug logging.

If your existing Caddy uses **Caddy Docker Proxy**, add these service-level
labels to `paperphase-auth` instead of adding a separate Caddyfile block:

```yaml
labels:
  caddy: auth.paperphase.oddorbit.software
  caddy.reverse_proxy: "{{upstreams 8080}}"
  caddy_ingress_network: caddy
  caddy.log.output: discard
```

Set `caddy_ingress_network` to the actual external network name if it differs
from `caddy`. Caddy must also join that network. The proxy discovers the labels
and creates the site automatically. Validate/start the edited Compose file with
`docker compose config --quiet` and `docker compose up -d --build`.

Check without any login parameters:

```sh
curl --fail https://auth.paperphase.oddorbit.software/healthz
```

It should return `ok`. No volumes, Google credentials or Pi addresses belong in
this container. If Caddy runs on the host instead, use a loopback-only published
port and adjust its upstream; the supplied Docker-network upstream will not
resolve from a host Caddy process.

## Create a Google OAuth client

1. Create/select a Google Cloud project and enable **Google Calendar API**.
2. Configure Google Auth Platform branding/audience. For a personal test, use an
   external audience in Testing and add every Google account you will link as a
   test user. Workspace administrators may also restrict third-party access.
3. Add these data-access scopes:
   - `https://www.googleapis.com/auth/calendar.calendarlist.readonly`
   - `https://www.googleapis.com/auth/calendar.events.readonly`
4. Create an OAuth client with application type **Web application**. Set its
   authorised redirect URI to exactly:
   `https://auth.paperphase.oddorbit.software/oauth/google/callback`
   There is no trailing slash. No Pi IP or hostname belongs in Google's redirect
   list. Authorised JavaScript origins are unnecessary for this redirect flow.
5. Save the client ID and client secret privately, and keep downloaded client
   JSON outside this source checkout. Use synthetic JSON for development/tests.
   `.paperphase/` is Git-ignored, but that does not prevent an assistant or other
   tools from reading files there. An outside-project location reduces accidental
   inclusion; enforced filesystem restrictions are needed to prohibit access.
   The planned JSON importer will let you supply the real file directly to the
   Pi; the current UI accepts the client ID and secret manually.

This initial implementation supports privately supplied clients. A shared
public OAuth client, consent-screen domain ownership and Google verification
for distribution require separate deployment/account review. In Google's
Testing status, Calendar refresh tokens normally expire after seven days;
reconnection is expected until the app's publishing arrangements are resolved.

## Configure the Pi and link accounts

Build/register the guest through `sh scripts/check.sh` and use the
`calendar.google` entry in `plugins/bindings.example.json` with the service's
existing registration file. Restart the service after adding its module.

Open the editor on the Pi's LAN using its real address, for example
`http://kitchen-pi.local:8080` or `http://192.168.1.53:8080`. Save/discard layout
and connection drafts before leaving for Google. Open **Data sources → Google
accounts**, enter the client ID/secret and hosted callback URL, and save setup.
A blank secret preserves a saved secret for the same client ID.

Enter an account label such as Personal or Work, select **Link Google account**,
and grant both read-only permissions. Google redirects the browser through the
public callback and back to the originating Pi. Stay on the same LAN/VPN, keep
the service running, and complete the flow within ten minutes. If a browser
requests local-network permission, allow access to your Pi. Changed addresses,
restarts, denied permissions or an expired attempt require starting again.

Add a **Google Calendar** connection, choose a linked account and calendar, set
its display label, timezone/window and privacy options, then save. Repeat for
other calendars or accounts. Fetching starts through the normal scheduler.
The Calendar agenda template shows that connection; Combined calendar agenda
shows all enabled Google connections with source labels. Existing component
editing applies to both templates. Disable connections to exclude them from the
combined agenda. Hidden title/location/description settings apply before caching.

Use **Reconnect** after consent expires, the grant is revoked, or calendar
membership changes. Remove an account's calendar connections before selecting
**Disconnect**. To revoke the provider-side grant too, remove Paperphase's OAuth
client access in the Google account's third-party connections settings.

Private credentials live beside the workspace as `workspace.google.json`; back
it up privately with the workspace and connection cache. Avoid sharing any of
these private files as diagnostics. No public API returns token/client-secret
values. Account and calendar labels/IDs remain visible in the LAN editor.

## Troubleshooting account linking

A failed callback shows a specific message when its Pi session is still known.
The same guidance is available under **Data sources → Google accounts**. A new
attempt clears the previous diagnostic. No credentials or provider payloads are
included in these messages or the log's fixed failure codes.

- **Calendar API disabled:** enable Google Calendar API in the project that owns
  the OAuth client, then try again. Consent can succeed before this API check.
- **Permissions missing:** approve both calendar listing and event reading.
- **OAuth client rejected:** check the Web application client ID and secret.
- **Code/grant rejected, missing or expired attempt:** start a new attempt from
  the same editor address, keep the Pi running and finish within ten minutes.
- **Calendar access denied:** check account permissions and Workspace restrictions.

Older builds discarded callback errors. They cannot explain a previous attempt;
retry once after updating to collect a safe diagnostic. A real account login is
still needed to establish the cause; the credentials-only network probe does
not validate Calendar API access or user consent.

## Verification still required

Desktop mocks do not replace a real Google login. After deployment, verify
healthz, consent, a selected calendar, a second account/Pi, renewal after access
expiry, revocation/reconnect and the seven-day Testing limit. Pi performance and
physical display integration are separate checks; the service still uses the
simulated display backend.

Protocol references: [Google web-server OAuth](https://developers.google.com/identity/protocols/oauth2/web-server),
[OAuth token expiry](https://developers.google.com/identity/protocols/oauth2),
[Calendar events](https://developers.google.com/workspace/calendar/api/v3/reference/events/list),
[Calendar list](https://developers.google.com/workspace/calendar/api/v3/reference/calendarList/list),
[Caddy access logging](https://caddyserver.com/docs/caddyfile/directives/log).
