# Flux usage statistics server

Optional first-party statistics for Flux 2.6.5. Requires PHP 8.3+, PDO SQLite and HTTPS. The desktop app sends nothing before an explicit acceptance. This server is separate from browser extensions, which do not send usage events.

The collector records an HMAC of a random installation identifier, UTC date, application version, platform and notice version. It never stores URLs, filenames, download contents, names or email addresses. Requests still expose the connecting IP to the hosting provider and its access logs; configure those separately. This is pseudonymous installation measurement, not an exact count of people.

## Plesk installation

Use `python scripts/package-statistics.py` to produce a private deployment ZIP and setup instructions under `output/2.6.5/`. That ZIP contains a random one-time setup token: keep it private and do not attach it to a public release. The public source contains no admin password, deployed key or database.

Upload `httpdocs/usage/` into the domain's `httpdocs/usage/` directory. Upload `private/flux-statistics/` beside `httpdocs`, outside the document root. The PHP account needs write access to that private directory. The public pages must be HTTPS, and the private directory must never be reachable through a web alias. If the hosting layout differs, set the PHP environment variable `FLUX_STATS_PRIVATE` to the private directory's absolute path.

Open `https://fluxdownload.org/usage/setup.php`, enter the one-time key from `private/flux-statistics/setup-token.txt`, choose an admin password of at least 14 characters, and enter the hosting provider and server country. Review the site's privacy notice, hosting agreements, access log retention and any applicable international transfer requirements before enabling collection. The server cannot determine these hosting-specific obligations for the operator.

After setup, use `https://fluxdownload.org/usage/` to sign in. The setup key is deleted after successful setup. Until setup is complete, the collector returns HTTP 503 and clients retry later; app downloads remain unaffected. Back up the private directory securely. Do not replace it with a fresh copy during ordinary website updates. To pause collection, set `enabled` to `false` in private `config.json`.

## Counting and retention

At most one active row per installation per UTC day. Raw rows expire after 35 days when the collector or dashboard is accessed. Configure a daily cron command `php /absolute/path/httpdocs/usage/prune.php` to enforce retention even without traffic. Counts include only consenting users who open and focus the app. Multiple installations can belong to one person; reinstalls and declined consent affect coverage. There is no public statistics endpoint.

Turning sharing off schedules deletion of that installation's retained rows; offline requests retry when the app is foregrounded. The identifier is not included in renderer status responses. Failed requests back off for six hours. The dashboard expires sessions after 30 minutes and limits failed login attempts using short-lived HMACs of IP addresses, deleted after 15 minutes. Those HMACs are not used for active-user counts.

## Verification

Run `php statistics-server/tests.php` and `python scripts/test-statistics-server.py`. The tests use temporary private storage and a local PHP server, never the production endpoint.
