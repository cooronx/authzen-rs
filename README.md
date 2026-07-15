<a id="readme-top"></a>

<!-- PROJECT SHIELDS -->
[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![MIT License][license-shield]][license-url]

<div align="center">
  <h3 align="center">authzen-rs</h3>

  <p align="center">
    A Rust SDK for the OpenID AuthZEN Authorization API 1.0, with Tower middleware support
    <br />
    <a href="https://github.com/cooronx/authzen-rs/issues">Report Bug</a>
    &middot;
    <a href="https://github.com/cooronx/authzen-rs/issues">Request Feature</a>
  </p>
</div>

<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li><a href="#about-the-project">About The Project</a></li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li>
      <a href="#usage">Usage</a>
      <ul>
        <li><a href="#client">Client</a></li>
        <li><a href="#features">Features</a></li>
        <li><a href="#scope-and-security">Scope and Security</a></li>
        <li><a href="#runnable-examples">Runnable Examples</a></li>
      </ul>
    </li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

<!-- ABOUT THE PROJECT -->
## About The Project

`authzen-rs` provides protocol models, an asynchronous HTTPS client, PDP traits,
and framework-neutral Tower integrations for both PEP and PDP applications. It
targets the [OpenID AuthZEN Authorization API 1.0][authzen-spec].

> [!NOTE]
> The crate does not implement authorization policies or bind users to a policy
engine. Authentication of callers and JWT validation are also intentionally
outside its scope.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- GETTING STARTED -->
## Getting Started

### Prerequisites

- Rust 1.85 or later
- Cargo

### Installation

Add the crate with its default client and Rustls TLS features:

```sh
cargo add authzen-rs
```

Enable additional integrations as needed:

```toml
[dependencies]
authzen-rs = { version = "0.1.0", features = ["tower"] }
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- USAGE EXAMPLES -->
## Usage

### Simple client example

```rust,no_run
use authzen_rs::prelude::*;

async fn run() -> Result<(), AuthZenError> {
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
```

### Paginated Search

Use a paginator when following Search results across multiple responses. It
keeps the original query fixed and changes only the opaque continuation token,
as required by Authorization API 1.0.

```rust,no_run
use authzen_rs::prelude::*;

async fn list_documents(client: &AuthZenClient) -> Result<Vec<Resource>, AuthZenError> {
    let request = ResourceSearchRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::query("document"),
    )
    .with_page(PageRequest::new().with_limit(100));

    let mut paginator = client.paginate_resources(request)?;
    let mut resources = Vec::new();
    while let Some(page) = paginator.next_page().await? {
        resources.extend_from_slice(page.results());
    }
    Ok(resources)
}
```

PDP Search adapters remain responsible for binding each opaque token to its
original query and rejecting continuation requests that change query values.

### Tower(axum) example

```rust,no_run
async fn get_document(
      Extension(decision): Extension<Decision>,
  ) -> &'static str {
      // The decision is inserted into the request extensions by AuthZenLayer.
      "document content"
  }

  #[tokio::main]
  async fn main() {
      let pdp = AuthZenClient::builder("http://localhost:8080")
          .build()
          .await
          .unwrap();

      let authzen = AuthZenLayer::new(pdp).request_mapper(|parts: &Parts| {
          let id = parts.uri.path().trim_start_matches("/documents/");

          Ok(EvaluationRequest::new(
              // In a real application, read the subject from a JWT or extension.
              Subject::new("user", "alice"),
              Action::new("read"),
              Resource::new("document", id),
          ))
      });

      let app = Router::new()
          .route("/documents/{id}", get(get_document))
          .layer(authzen);

      let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
      axum::serve(listener, app).await.unwrap();
}
```

### Features

| Feature | Purpose |
|---|---|
| `client` | Async AuthZEN HTTPS client and strict metadata discovery |
| `server` | PDP and Search implementation traits |
| `tower` | Tower integration: PEP middleware with `client`, PDP HTTP services with `server` |
| `rustls-tls` | Rustls transport; enabled by default |
| `native-tls` | Native TLS transport |
| `tracing` | Internal diagnostics without exposing PDP errors to clients |

### Runnable Examples

Every example runs without an external service:

```sh
cargo run --all-features --example client
cargo run --all-features --example custom_pdp
cargo run --all-features --example tower_pep
cargo run --all-features --example tower_pdp
```

The client example prints its request in offline mode. To send it to a real,
spec-compliant HTTPS PDP:

```sh
AUTHZEN_PDP_URL=https://pdp.example.com \
AUTHZEN_TOKEN=replace-me \
cargo run --all-features --example client
```

Run `./scripts/check-examples.sh` to execute all four examples and verify their
observable output.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTRIBUTING -->
## Contributing

Contributions are welcome. To propose a change:

1. Fork the project.
2. Create a feature branch (`git checkout -b feature/amazing-feature`).
3. Commit your changes (`git commit -m 'Add an amazing feature'`).
4. Push the branch (`git push origin feature/amazing-feature`).
5. Open a pull request.

Please open an [issue][issues-url] for bugs and feature requests.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- LICENSE -->
## License

Distributed under the MIT License. See [`LICENSE`](LICENSE) for more information.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTACT -->
## Contact

cooronx — [2197083441@qq.com](mailto:2197083441@qq.com)

Project link: [https://github.com/cooronx/authzen-rs](https://github.com/cooronx/authzen-rs)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- ACKNOWLEDGMENTS -->
## Acknowledgments

- [OpenID AuthZEN Working Group][authzen-spec]

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
[contributors-shield]: https://img.shields.io/github/contributors/cooronx/authzen-rs.svg?style=for-the-badge
[contributors-url]: https://github.com/cooronx/authzen-rs/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/cooronx/authzen-rs.svg?style=for-the-badge
[forks-url]: https://github.com/cooronx/authzen-rs/network/members
[stars-shield]: https://img.shields.io/github/stars/cooronx/authzen-rs.svg?style=for-the-badge
[stars-url]: https://github.com/cooronx/authzen-rs/stargazers
[issues-shield]: https://img.shields.io/github/issues/cooronx/authzen-rs.svg?style=for-the-badge
[issues-url]: https://github.com/cooronx/authzen-rs/issues
[license-shield]: https://img.shields.io/github/license/cooronx/authzen-rs.svg?style=for-the-badge
[license-url]: https://github.com/cooronx/authzen-rs/blob/main/LICENSE
[authzen-spec]: https://openid.github.io/authzen/
