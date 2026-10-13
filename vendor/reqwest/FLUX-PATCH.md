# TCP socket buffer patch

Source: reqwest 0.12.28, crates.io package, MIT OR Apache-2.0.

The local patch exposes `ClientBuilder::tcp_socket_buffers(send, receive)` and
forwards the values to hyper-util's TCP connector before socket connection.
It does not change TLS, certificate checks, redirects, DNS, proxies, or QUIC.
The operating system may clamp requested buffer sizes to its configured limits.

The only modified upstream files are `src/async_impl/client.rs` and `src/connect.rs`.
Remove the workspace patch when the upstream builder exposes equivalent controls.
