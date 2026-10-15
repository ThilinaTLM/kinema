# Adapter contract and endpoint evidence

Verified against official documentation and public addon manifests on 2026-10-15:

- <https://api.real-debrid.com/>: REST `1.0/torrents/addMagnet`,
  `torrents/info/{id}`, `torrents/selectFiles/{id}` (`files=<chosen provider ID>`,
  empty 204 response), `unrestrict/link`, and GET `user` for credential verification.
  No instant-availability API is used.
- <https://docs.alldebrid.com/>: POST `v4/magnet/upload`,
  `v4.1/magnet/status`, `v4/magnet/files`, `v4/link/unlock`, and
  `v4/link/delayed`, and GET `v4/user` for credential verification. All requests use
  the officially documented `Authorization: Bearer <API key>` header, not credentials
  in query strings. The docs' files example has a v4.1 URL, but its HTTP
  Request section and endpoint index specify v4; this adapter uses v4.
  Delayed-link polling respects the documented minimum five-second interval.
- <https://torrentio.strem.fun/manifest.json>: public noncredentialed addon,
  version 0.0.15, movie/series IMDb stream resources.
- <https://peerflix.mov/manifest.json>: public zero-configuration addon,
  version 2.7.4, movie/series IMDb stream resources. This is also the archive's
  noncredentialed default. The configurable credential-bearing addon is not used.

## Safety and integration

`DebridClient::check_account(&AtomicBool) -> Result<(), DebridError>` verifies the
key with the authenticated user endpoint and discards account details. RD requires
a positive numeric account ID and nonempty username; AD requires a nonempty
username and boolean `isPremium` in the success envelope. Free/nonpremium accounts
still authenticate successfully: this is not a premium-entitlement or playback
readiness check. HTTP 401/403 and AD `AUTH_*` errors produce `CredentialRejected`;
missing OS-store entries remain an application/credential-store concern. Tests
cover both user endpoints, success, rejected credentials, malformed account
responses, transport/service errors, cancellation, and safe error text.

`SensitiveUrl::for_test(Url) -> Self` is available only under `cfg(test)` for local
HTTP media fixtures. Production URL validation remains HTTPS-only.

These are blocking adapters, not UI services. The owner must call them from its
bounded worker, own cancellation, and generation-check returned results. The
adapters create no detached tasks. Indexer selection is explicit; there is no
federation or credential forwarding to indexers.

Every HTTP request has a 20-second timeout (8-second connect timeout), no redirects,
and a 4-MiB response cap. Indexers accept at most 1,000 rows. Provider catalogs
accept at most 4,096 files, with a 32-level AllDebrid tree and 4,096-byte paths.
Readiness has at most 30 polls; sleeps check cancellation every 100 ms. An in-flight
blocking HTTP call observes cancellation on return, bounded by its timeout, not
instantaneously. `IndexerClient::fetch(indexer, target, &AtomicBool)` uses the
owner's cancellation flag for HTTP and checks it before starting and before/after
stream parsing. Canceled fetches return `SourceError::Canceled`, never candidates
for persistence. The owner must still generation-check results to close any race
between return and application.

`SourceResults.omitted` counts direct-URL-only unsupported rows and duplicate
hash/file-index rows. Empty `streams` succeeds; invalid envelopes, invalid hashes,
and invalid structured numeric hints fail rather than masquerading as empty.
Magnet parsing accepts only the shared 40-hex v1 hash contract.

Release labels prefer modern `description`, then legacy `title`, then addon `name`.
Quality uses the combined structured quality/addon/release text; standalone `4K`
normalizes to `2160p`. Structured `seed` is accepted alongside `seeders`, and
`sizebytes` alongside `size`/`videoSize`/`torrentSize`. Structured values take
precedence. When absent, only the exact `👤` seed and `💾` size markers are parsed
from at most 2,048 release-label characters. Duplicate markers, malformed values,
overflow, and unknown units are ignored. Size estimates use literal SI KB/MB/GB/TB
and IEC KiB/MiB/GiB/TiB with at most three decimal places. Upstream addons may label
binary quantities as GB, so these rounded fallback estimates are explicitly
unverified UI hints, not measured bytes. **All indexer size/seed/quality data is
unverified: never use it for cache admission, provider file selection, or stable
media identity.** Provider-confirmed actual file path/size remain authoritative.

An opt-in real `IndexerClient::fetch` smoke for IMDb `tt0133093` succeeded for both
public endpoints on 2026-10-15: Torrentio returned 97 candidates / 0 omitted, and
Peerflix 21 / 0 omitted. The lead's saved public snapshots parsed to the same counts.
Torrentio had 97 size and seed hints; Peerflix had 18 size hints and 21 seed hints
(three upstream sizes were null). Counts are observations, not stable guarantees.
The ignored `public_indexers_live_opt_in` test repeats only public noncredentialed
HTTP requests, not debrid authentication or playback. The machine-specific external
snapshot diagnostic test was removed; portable inline fixtures remain.

Real-Debrid preparation only returns the metadata/catalog; it never selects files.
Resolution selects only the chosen provider file ID when the job is waiting for
selection, then maps links strictly by the provider's selected-file order, not
torrent index or file ID. Downloaded jobs are never reselected: an unselected file
returns `FileNotFound`, leaving any new preparation decision to the coordinator.
AllDebrid flattens
its actual file tree and returns opaque local ordinal IDs, never link-valued IDs.
Resolution reloads the tree and matches actual path and size, so renewed access
retains `hash + actual file path + size`. File-index/name hints are not guessed into
provider IDs; conservative file choice belongs to the coordinator/core policy.

Zero or unavailable size cannot resolve. Only validated HTTPS media URLs without
userinfo/fragments are accepted. Credentials, provider response bodies, magnets,
and resolved URLs never appear in error text. Prepared/resolved Debug is redacted.
No credentials or remote links are persisted here. `expires_at` remains `None`:
these endpoints do not reliably disclose signed-link expiry. Renewal can repeat
prepare, match the original actual path/size, and resolve again; job/file IDs are
not promised stable across preparation. Jobs are retained at the provider; the
adapter does not delete a user's remote torrent on local cancellation.

Local HTTP fixtures test both indexer routes, omission/deduplication, metadata
hints, both provider flows, nested/multiple files, selected-link mapping, renewed
identity, HTTP/envelope auth failures, malformed responses, body limits,
readiness/delayed timeouts, cancelable sleeps, and redaction. No credentialed live
provider account, real playback, or OS behavior is claimed by these tests.
