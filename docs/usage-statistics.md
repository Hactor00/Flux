# Optional usage statistics · Flux 2.6.5

Flux works without sharing statistics. The desktop asks before creating a measurement identifier or contacting the collector. Browser extensions do not send these events. This setting is separate from browser native-transfer permission and from GitHub update checks.

## Choice and reminders

- **Yes, share:** enables sharing and creates a random UUID for that installation.
- **Later · 7 days:** leaves sharing off and asks again after seven days.
- **Not now · 45 days:** leaves sharing off and explicitly schedules the next reminder after 45 days.
- **Settings → Usage statistics → Do not ask again:** disables reminders permanently until the user changes that preference.

Closing the first dialog is equivalent to Later. Turning sharing off in settings stops active events and disables reminders. It also queues deletion of that installation's retained records. Deletion retries when the app is foregrounded and online. Re-enabling sharing uses a new identifier; previous pending deletion requests are retained. There is no penalty or feature restriction for declining.

## Data and purpose

The app sends a JSON POST to `https://fluxdownload.org/usage/collect.php` at most once per UTC day while the main window is visible and focused:

| Field | Purpose |
| --- | --- |
| Random installation UUID | Deduplicate daily and monthly active installations |
| App version | Understand adoption of releases |
| Operating system (`windows`, `linux`, `macos`) | Plan platform work; this does not announce a macOS release |
| Notice version | Record which explanation was accepted |
| UTC date, assigned by the server | Group activity by day |

No download links, filenames, downloaded contents, browser history, cookies, authorization headers, name or email are included. The public renderer status does not expose the identifier. The server stores an HMAC of the UUID instead of the raw value. These remain pseudonymous usage records, not a guarantee of anonymity. IP addresses are visible to the hosting provider and may appear in its access logs; the collector database does not store them. The operator must configure infrastructure logs, security, hosting and any applicable transfer requirements separately.

Rows are retained for 35 days. The private dashboard shows daily, seven-day and thirty-day distinct installations. One person can use several installations, and people who decline are absent. Counts are not exact users, downloads or all installed copies. They can also be affected by reinstalls, network availability and forged events to a public collector.

## Access, withdrawal and hosting

Project contact and operator: **Rıza Altıner**, `rizaa.altiner@gmail.com`. Users can withdraw through Settings and contact the maintainer about retained data. Details are also available at [fluxdownload.org/privacy](https://fluxdownload.org/privacy/). A consent dialog alone does not determine compliance for every jurisdiction or hosting arrangement.

The open-source [PHP server](../statistics-server/README.md) is delivered separately as an upload-ready package. It accepts no events until the operator completes one-time setup, enters hosting information, sets a private admin password and reviews the notice/infrastructure settings. The dashboard is at `https://fluxdownload.org/usage/`, accessible only after login. The setup token, server secret, database and password hash live outside the web root; they never enter public GitHub artifacts.

Failed requests back off for six hours. Downloads and updates do not depend on the statistics service. A user who accepts before server deployment remains opted in locally, but no successful measurement is recorded until the service is installed.
