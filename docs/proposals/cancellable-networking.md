# Cancellable networking

**Status:** Draft · **Area:** runtime

## Problem

Provider and acquisition HTTP calls block, and can't be interrupted. A cancelled
request keeps its worker busy until its timeout. Shutdown can't join fetch
threads, and renewal callbacks must police their own I/O. Fixes the
[known debt in runtime](../architecture/runtime.md#background-work-rules).

## Current state

Blocking clients run on bounded worker pools, with connect and total timeouts.
Cancellation only discards the results. A tokio runtime is already a dependency
for some components.

## Options

1. **An async HTTP client on one owned runtime.** Workers hold `JoinHandle`s and
   abort them when cancelled. The pure adapters stay synchronous at the edges
   by returning futures.
2. **Keep blocking clients, but make reads interruptible.** Close the socket on
   cancellation (a per-request handle for the connection). Less change, but it
   depends on the client.
3. **Keep the current model,** and shorten timeouts for low-priority work.

## Open questions

- Should one runtime serve the catalog, media and debrid, or each get its own?
- How does this interact with the generation and slot model in the catalog
  worker?
