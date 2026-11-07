# Direct torrent playback

**Status:** Draft · **Area:** media

## Problem

Without a debrid service, users cannot play anything. A built-in torrent engine
would let them stream directly.

## Current state

- There is an isolated proof of concept, `examples/rqbit_poc.rs`, behind the
  opt-in `rqbit-poc` feature, using `librqbit =9.0.1`. It is not wired into the
  app.
- What the proof of concept showed:
  - The API can stream to the loopback server.
  - A seek into the middle of a file took about 8 s.
  - `Session::stop` does not guarantee a join, so ownership and cancellation
    need a design.
  - The crate is Apache-2.0, but its dependency graph has not been audited.

## Recommendation

- Make the torrent engine an **acquisition source** for the existing media
  service. It writes into the same payload and manifest, and serves through the
  same server and leases. It needs piece prioritization driven by reader hints.
- Users must **opt in explicitly**. The opt-in has to explain that peers can see
  the user's IP address. It also needs limits on upload and seeding, and clear
  cancellation and storage management.
- It follows the same runtime rules as everything else: an owner, generations,
  bounded resources and no detached threads.

## Open questions

- Can the seek latency be brought close to the debrid path?
- Should seeding stop when playback ends, or be capped by ratio?
- What does the license and privacy review need to cover before this can ship?
- How should Windows and macOS behave (firewalls, port mapping)?
