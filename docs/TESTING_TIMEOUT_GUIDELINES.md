# Testing timeout guidelines

## Command execution

- Always run agent-triggered commands with an explicit timeout.
- Prefer narrow test targets before broad suites.
- If a command can block on network, locks, or graceful shutdown, assume it eventually will and bound it.

## Async and integration tests

- Every polling loop must have a bounded retry count or a wall-clock timeout.
- Every network wait should be wrapped in an explicit timeout.
- Long-lived streams such as SSE, websockets, and subscriptions must be closed or dropped before test shutdown.
- Shutdown paths must not depend on an open client connection eventually closing by itself.

## Suggested Rust patterns

- Use `tokio::time::timeout` around waits for events, chunks, and background completion.
- Normalize line endings in streamed test helpers when protocols may emit either `\n\n` or `\r\n\r\n`.
- For stress/backpressure tests, publish directly to internal buses when possible instead of generating hundreds of slow HTTP requests.

## Practical defaults

- Single event wait: 1-5 seconds
- Import/background job completion: 10-30 seconds total budget
- Full integration test: keep under 60 seconds unless there is a documented reason otherwise
- Agent-run targeted test command: start with a 120-second terminal timeout and reduce scope before increasing time
