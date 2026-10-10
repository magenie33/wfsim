# Accounts

An account is optional. Everything WFSim does works without one, and a reader
who never signs in has the site exactly as it was. What an account adds is a
place for a person's own things to live beyond one browser.

## The model

**AN ACCOUNT IS A UUID, reached through four SLOTS**: Google, Discord, GitHub,
and an email address with a password — the first-party way in. At most one of
each. So there are two ways to sign in: a third party in one click, or an
address and its password.

**MAIL GOES OUT ONLY TO PROVE AN ADDRESS** — to register one, to link one, to
reset its password. Signing in with a password sends nothing. No account exists
until a registration's code comes back, so an address nobody proved never
becomes an account.

| rule | where it is held |
| --- | --- |
| a slot is `(provider, subject)`: the provider's own id for the person; for email, the address lowercased | `identities` primary key |
| one slot per provider per account | `UNIQUE (account, provider)` |
| a slot belongs to one account at most | the primary key — so nothing can be merged by accident |
| the account lives while a slot is filled | trigger `identities_last_slot`, so every path that empties the last slot deletes the account, and by cascade its sessions and documents |
| a new account arrives with its first slot | one batch in `arrive` — no moment exists with a UUID and no way to reach it |

## Agents

**AN AGENT GETS A KEY AT ONCE, AND AN ACCOUNT ONLY WHEN ITS PERSON SAYS SO.**
`worker/agents.js` issues the key, and draws `/auth.md` and the two OAuth
metadata documents from the same constants as its endpoints.

| rule | where it is held |
| --- | --- |
| registering needs nothing, is limited per address, and returns the key once; only its hash is kept | `register`, `agent_keys.key_hash` |
| an unclaimed key nobody used for 90 days is dropped | `register` |
| a claim mails a code to the address — or, where no account holds it, a mail saying so — and answers the same either way | `claim` |
| the right code within ten minutes and five tries binds the key to the account | `claimComplete` |
| a claimed key acts for its account on `/api/cloud/*` and nowhere in billing | `cloudRoute` → `agentAccount` |
| the account page lists its agents and disconnects one in a click; an agent may revoke its own key | `/api/account/agents`, `AGENT_PATHS.revoke` |
| deleting the account deletes its agents' keys; the export lists them | `ON DELETE CASCADE`, `agentsOf` |

## Names

**EVERY ACCOUNT HAS A USERNAME**, the site's handle for it, and may have a
display name. Anything an account signs stores the account's id, so a rename
shows everywhere at once.

| rule | where it is held |
| --- | --- |
| a new account is born `user_` and six random characters; every existing one was given one by `accounts-0003-usernames.sql` | `bornUsername`, `arrive` |
| a username is 3–20 of `a-z`, `0-9`, `_`, typed in any case and kept lowercase, and unique | `USERNAME`, index `accounts_username` |
| `user_` names and a reserved list are the system's; nobody picks one | `USERNAME_BORN`, `USERNAMES_RESERVED` |
| the first change is at once; each later one waits a day after the last | `renameAfter`, `username_changed_at` |
| a name given up, or a deleted account's, is held seven days for its old owner, who may take it back | `username_holds`, `holdUsername` |
| a display name is free text, at most 32 characters, unique to nobody; empty shows the username | `profile` |
| a name says who someone is and points nowhere else: no link, contact details, offer to trade or claim to speak for WFSim, and no word from the bundled list; any handle passes, a Bilibili one included | `nameBlocked` in `worker/names.js` |
| a display name set before a rule that refuses it is kept and hidden: others see the username, and the account page says why | `shownName`, `display_name_hidden` |

`POST /api/account/profile` `{username?, display_name?}` changes either; the
`/account` page's Profile block is the form, and the top bar shows the display
name with `@username` under it.

**THE SERVER NEVER MERGES.** An identity already on another account is refused
(`taken`), never moved. A matching email address means nothing: a Google
account whose address equals a linked email is a different slot and, signed in
first, a different account. Two accounts become one only by their owner
removing a way in from one and adding it to the other.

**Replacing a slot is an UPDATE**, not a delete and an insert: the slot is
never empty, so the last-slot trigger cannot fire half way through.

**Removing the last slot is deleting the account.** `/api/account/unlink`
refuses it with `last_slot` unless the call says `delete_account: true`, and
the panel asks for that inline — no native dialog.

Every way in ends in one function, `arrive` in `worker/accounts.js`, so no way
in can have its own idea of what linking means.

## Where it lives

`worker/accounts.js` behind the site's worker, `worker/accounts.sql` in its own
D1 database, `wfsim-accounts` — apart from `wfsim` (docs/NAMING.md §8), because a
point-in-time restore of one must not roll the other back, and because nothing
personal may reach the public `library-backups` branch, which reads `wfsim`.
D1 Time Travel is its backup: any minute of the last 30 days.

## On the page

| surface | what it is |
| --- | --- |
| top bar | ONE always-visible entry, including on phones: "Sign in", or an avatar whose menu holds settings, the reader's builds and sign-out |
| `/login` | third parties in one click, then an email and a password; "Forgot password?" beside the password |
| `/signup` | the same ways in; an email and a password, then a page for the mailed code — six boxes, a paste fills them, the last digit submits, a resend after a minute |
| `/reset` | an email, then the code and a new password |
| `/account` | settings in `.block` sections — ways to sign in, email and password, data and privacy, and a danger zone that asks for `DELETE` typed before it deletes |
| `/account/sync` | the usage per pool against the allowance, the upload default, and every item this browser holds — filtered, searched, newest first or by weapon — with its sync switch, one or several at once; nothing on it deletes |

These pages, and any page an extension mounts (docs/UI.md §Extensions), are
one `<main id="auth-page">`, drawn by `web/src/static/app/17-account.js` from
the route. Nothing draws where
`/api/account` names no way in, so the dev server, the desktop shell and a site
whose secrets are not set show no account control. A sign-in page returns the
reader to `?return=`, and an OAuth round trip from one comes back through
`/login` so a refusal is said in the card; elsewhere an outcome is a toast.
Disconnecting the last way in is refused and sends the reader to the danger
zone, so deleting an account is always the one deliberate act. Links that leave
the app (an OAuth start, `/privacy`) carry `data-native`, which the client-side
router leaves to the browser.

| endpoint | does |
| --- | --- |
| `GET /api/auth/<google\|discord\|github>/start?intent=login\|link&return=/path` | the OAuth round trip, PKCE and a signed state cookie |
| `GET /api/auth/<provider>/callback` | back to `return` with `auth=<outcome>` or `auth_error=<reason>` |
| `POST /api/auth/email/register` `{email, password}` | mails a code; `email_taken` if the address has an account |
| `POST /api/auth/email/link` `{email, password}` | signed in: mails a code to fill or replace the email slot |
| `POST /api/auth/email/reset` `{email}` | mails a code if the address has an account, and answers the same if not |
| `POST /api/auth/email/verify` `{email, code, password?}` | completes what the code was mailed for; five tries, ten minutes |
| `POST /api/auth/email/login` `{email, password}` | no mail; `wrong_credentials` alike for a wrong address or password, `locked` after five in fifteen minutes |
| `POST /api/account/password` `{current?, password}` | signed in: `current` where the slot has a password |
| `GET /api/account` | the account and its slots, and which ways in are configured |
| `POST /api/account/unlink` `{provider, delete_account?}` | empties a slot |
| `POST /api/account/delete`, `/api/account/export`, `/api/auth/logout` | as named |

A state-changing call is JSON from this origin; the session cookie is
`HttpOnly; Secure; SameSite=Lax`, a random token whose hash is the table's key.

## The private worker

**WHAT THIS REPOSITORY DOES NOT HOLD runs in a separate private worker,
`wfsim-cloud`, reached only through the site worker's `CLOUD` service binding.**
`worker/cloud.js` is the whole of it here, and it forwards:

| rule | where it is held |
| --- | --- |
| every path under `/api/billing`, `/api/cloud/` and `/api/stripe/` goes to the private worker | `cloudPath` |
| the signed-in account travels as `x-wfsim-account`; a browser's own header is dropped, and so is the cookie | `cloudRoute` |
| a state-changing paid call from another site never leaves this worker | `cloudRoute` → `sameSite` |
| Stripe's webhook goes through as sent, as nobody | `cloudRoute` |
| an account is deleted only once the private worker has ended its subscriptions | `cloudEnd`, `billing_open` |
| with no binding, billing reads as off and nothing is ended | `cloudRoute`, `cloudEnd` |

**EVERY ACCOUNT SYNCS**, up to the allowance the private worker states. The
page's half is public (docs/UI.md §"Build sync"); the server's is
`/api/cloud/sync` in the private worker. The `/account` page's data block says
where sync stands and offers the one action the state calls for; the export
carries what it holds.

What the private worker adds to the page — its own pages, a link in the
settings navigation, a line under the danger zone — it mounts (docs/UI.md
§Extensions). `/terms` and `/refunds` are plain pages beside `/privacy`, their
bodies filled by the site's worker from the private one.

## Privacy

Privacy at the level of an ordinary service: keep what the account needs,
state it at `/privacy` (written by `build_site_app.py`, `PRIVACY_BODY`), and
change that page in the commit that changes what is kept.

- A slot keeps the provider's id, a label shown to its owner (an address or a
  name) and when it was linked. Nothing else from a provider; the access token
  is read once for the id and dropped.
- A password is at least 8 characters and nothing more is asked of it. It is
  kept as PBKDF2-SHA256 at 100,000 rounds — a Worker's ceiling — over an HMAC
  keyed with `AUTH_SECRET`, so a copy of the table alone cannot test a guess.
  A reset or a change signs every other browser out.
- No IP address is written. The rate limit on the email endpoints keys on it in
  memory.
- An email address never travels in a URL and is never logged. The mail
  service's own delivery log holds the recipient for up to 30 days; its message
  preview stays off, so no code is retained there.
- A browser that computes the board's scores keeps a random id it made and
  sends it with each one (`verifiers`, docs/BOARD.md §"Compute orders"). A
  signed-in browser is claimed for its account (`devices`), so its work counts
  there (docs/BOARD.md §"Contribution"). Its points are on the public ranking
  anonymously; its display name (else its username) is shown only once the
  account agrees (`contribution_choice`). It is never joined to a submission.
- **An account is never joined to the usage count or to a board submission.**
  Both pages promise that nothing about the reader travels with them, and the
  session cookie reaching those endpoints is never read by them.

## Clan invites

`/clan` is the in-game clan's door: a clan joins by invitation only, so a
signed-in reader leaves an in-game name there and is invited in game.
`worker/clan.js` sends it with the username as one Discord message to the
owner, through the paid half's bot (`cloudTell`, `/internal/tell`), and stores
nothing — so there is nothing to export or delete. A name sent again from the
page says it replaces the first; `CLAN_LIMIT` allows two per account a minute,
so a typo is corrected at once.

## Offered screenshots

A riven screenshot is read in the reader's browser (`25-riven-ocr.js`) and does
not leave it. What it read becomes a new card, and a review window holds the
screenshot and the riven editor itself until OK; Cancel deletes the card.
Ticking **Send this read and your corrections** there sends, on OK, the rectangle around the lines that named a
stat, every line read, what they first read as and what was confirmed — to
`/api/ocr/sample` (`worker/ocr_samples.js`), stored in the bucket under
`ocr-samples/<day>/`. An edit on the card afterwards is a change of mind, not a
correction, and is never sent.

- No account, no address, no identifier is stored, and the session cookie is
  not read. A sample therefore cannot be found again for its sender, which is
  why `/privacy` says it cannot be withdrawn.
- 365 days, by the bucket's lifecycle rule (`wrangler.jsonc` states it).
- The box is unticked until the reader ticks it once; this browser then
  remembers either answer (`wfsim-ocr-give`, a preference like the board's) and
  shows it on every window, where it is changed.

## Setup

What the owner does once, and what each gives:

| what | where | then |
| --- | --- | --- |
| `AUTH_SECRET` | random, `wrangler secret put AUTH_SECRET` | signs the OAuth state and the code hashes; nothing works without it |
| Google | Google Cloud console, OAuth client, redirect `https://wfsim.app/api/auth/google/callback` | `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` |
| Discord | Discord developer portal, OAuth2, redirect `…/api/auth/discord/callback` | `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET` |
| GitHub | GitHub OAuth app, callback `…/api/auth/github/callback` | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` |
| Email | Cloudflare Email Service: onboard `wfsim.app`, preview off | a `send_email` binding named `EMAIL` in `wrangler.jsonc`, sender `login@wfsim.app` |

A way in whose secrets are absent is simply not offered.

`scripts/check_accounts.mjs` runs these rules against the real schema in node's
own SQLite: no network, no browser.
