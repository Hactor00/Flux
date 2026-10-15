# Piece-sized rate limiter burst

Source: librqbit 9.0.1, crates.io package, Apache-2.0.

Only `src/limits.rs` changes upstream behavior. The governor quota retains the
configured bytes per second while allowing a burst of at least one 16 KiB piece.
This permits a shared bandwidth allocation below 16 KiB/s without rejecting
piece reservations as exceeding the quota's burst capacity. Per-task UI limits
retain their existing minimum; internal allocations can be smaller.
