# authzen-rs

Rust SDK for the [OpenID AuthZEN Authorization API 1.0](https://openid.github.io/authzen/).

The crate provides protocol models, an asynchronous HTTPS client, PDP traits, and framework-neutral Tower integrations for both PEP and PDP applications. It does not implement authorization policies or bind users to a policy engine.

## Features

| Feature | Purpose |
|---|---|
| `client` | Async AuthZEN HTTPS client and strict metadata discovery |
| `server` | PDP and Search implementation traits |
| `tower-pep` | Authorization middleware for protected applications |
| `tower-pdp` | AuthZEN HTTP endpoint services for PDP applications |
| `tower` | Enables both Tower integrations |
| `rustls-tls` | Rustls transport; enabled by default |
| `native-tls` | Native TLS transport |
| `tracing` | Internal diagnostics without exposing PDP errors to clients |

## Client

```rust,no_run
use authzen_rs::prelude::*;

# async fn run() -> Result<(), AuthZenError> {
let client = AuthZenClient::builder("https://pdp.example.com")
    .bearer_token("token")
    .discover()
    .build()
    .await?;

let decision = client.evaluate(EvaluationRequest::new(
    Subject::new("user", "alice"),
    Action::new("read"),
    Resource::new("document", "123"),
)).await?;

if decision.allowed() {
    // Continue the protected operation.
}
# Ok(())
# }
```

Metadata discovery is strict: discovery failures and `policy_decision_point` mismatches are errors. Without `.discover()`, the client uses the standard `/access/v1/...` paths. Requests time out after five seconds and responses are limited to 4 MiB by default; both limits are configurable.

## Scope and security

- HTTPS is required by the AuthZEN 1.0 binding.
- A denied authorization is HTTP 200 with `decision: false`, not a transport error.
- `signed_metadata` is preserved as a raw JWT, but v0.1 does not validate JWS signatures.
- PDP implementation errors are hidden from HTTP clients by default.
- Authentication of callers and JWT validation are intentionally outside this crate's scope.

See the runnable examples for custom PDP and Tower integration.

## Runnable examples

Every example runs without an external service:

```sh
cargo run --all-features --example client
cargo run --all-features --example custom_pdp
cargo run --all-features --example tower_pep
cargo run --all-features --example tower_pdp
```

The Client example prints its request in offline mode. To send it to a real, spec-compliant HTTPS PDP:

```sh
AUTHZEN_PDP_URL=https://pdp.example.com \
AUTHZEN_TOKEN=replace-me \
cargo run --all-features --example client
```

Run `./scripts/check-examples.sh` to execute all four examples and verify their observable output.
