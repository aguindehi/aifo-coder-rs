AIFO Coder Tool Exec Protocol (Shim ↔ Proxy)

Overview
- This document describes the HTTP-based protocol between the aifo-shim client and the in-agent proxy server.
- Two protocol versions are supported:
  - v1: buffered response with Content-Length and X-Exit-Code header.
  - v2: streaming response using HTTP/1.1 chunked transfer encoding with X-Exit-Code in the response trailers.

Version negotiation
- The client sets the header: X-Aifo-Proto: 1 or X-Aifo-Proto: 2
- The server requires the header when Authorization is valid. If unsupported/missing:
  - Respond with 426 Upgrade Required, body: "Unsupported shim protocol; expected 1 or 2\n"

Authentication
- The client sends Authorization: Bearer <token> (or any scheme containing the token as the last whitespace/equals-separated token).
- The server validates the token and returns 401 Unauthorized when invalid/missing.

v1 (Buffered)
- Request: POST with Content-Length and form-encoded body (tool, cwd, arg=... repeated).
- Response:
  - Status: 200 OK on success.
  - Headers: Content-Type: text/plain; charset=utf-8, X-Exit-Code: <int>, Content-Length: <len>, Connection: close
  - Body: concatenated stdout+stderr of the executed command.
- Behavior: server buffers entire output before sending, then closes the connection.

v2 (Streaming)
- Request:
  - POST with headers:
    - X-Aifo-Proto: 2
    - TE: trailers (recommended for curl so trailers appear in -D header file)
  - Body: same as v1 (form-encoded).
- Response:
  - Status: 200 OK on success.
  - Headers:
    - Content-Type: text/plain; charset=utf-8
    - Transfer-Encoding: chunked
    - Trailer: X-Exit-Code
    - Connection: close
  - Body: streamed as HTTP chunks; server merges stderr into stdout ordering by wrapping the exec with sh -lc '<cmd> 2>&1'.
  - Trailers:
    - X-Exit-Code: <int>
- Behavior: the server streams output as it is produced; on process exit it emits the final zero-length chunk and trailers.

Tool routing and allowlists
- The proxy maps tools to sidecars with dynamic fallback for common dev tools:
  - Dev tools: make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++
    - Preferred order: c-cpp, rust, go, node, python
    - Selects the first running sidecar that reports the tool available (command -v inside the container).
  - Rust tools: cargo, rustc → rust sidecar.
  - Node/TS tools: node, npm, npx, tsc, ts-node → node sidecar.
  - Python tools: python, python3, pip, pip3 → python sidecar.
  - Go tools: go, gofmt → go sidecar.
- Allowlists per sidecar include relevant dev tools to allow execution where present.

Notes
- The server does not set a write timeout in streaming mode to avoid mid-body truncation.
- In verbose mode, server logs are printed on stderr with careful line handling (flush + clear line).
- For buffered (v1) responses, the server adds a leading/trailing newline in verbose mode to avoid UI line wrap artifacts.
- The rust sidecar sets CARGO_HOME, exports CC=gcc and CXX=g++, and relies on the image PATH. The PATH is not overridden via -e to avoid losing system paths.

Client (shell shim) behavior
- Uses curl -sS --no-buffer with:
  - -D "$tmp/h" to capture headers and trailers
  - -H "X-Aifo-Proto: 2" and -H "TE: trailers" to enable streaming
- Unix socket transport (Linux):
  - If AIFO_TOOLEEXEC_URL starts with unix://path/to.sock, the shim passes --unix-socket path/to.sock to curl and uses http://localhost/exec as the request URL.
- Streams response body directly to stdout (no -o)
- Extracts X-Exit-Code from headers (trailers appear at the end of the header file)
- Exits with that code (falls back to 1 if header/trailer missing)

Backward compatibility
- The proxy supports both v1 (buffered) and v2 (streaming) protocols; clients choose via X-Aifo-Proto.
- If Authorization succeeds but the client omits or sets an unsupported protocol, the server responds with 426 Upgrade Required and a clear message.
- v1 remains the default for legacy clients; v2 is recommended for improved UX via live streaming and exit code trailers.

Implementation status
- Protocol v2 (streaming with chunked transfer + X-Exit-Code trailer) is implemented in both TCP and unix-socket proxy paths.
- The shim now streams output live using curl --no-buffer, requests v2 via X-Aifo-Proto: 2, and supports unix:// URLs via --unix-socket.
- Backward compatibility with v1 is preserved (buffered response with Content-Length and X-Exit-Code header).
- Dynamic tool routing is implemented: dev tools (make, cmake, ninja, pkg-config, gcc, g++, clang, clang++, cc, c++) route to the first running sidecar that provides them (preferring c-cpp, then rust, go, node, python).
- Allowlists expanded accordingly; rust sidecar exports CARGO_HOME, CC, CXX and relies on its image PATH (no PATH override).
- Verbose logging is line-safe (flush + clear line) and does not interfere with streamed output.
