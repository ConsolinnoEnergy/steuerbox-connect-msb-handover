# ADR-0009: mTLS terminates inside the application

- **Status:** Accepted
- **Context:** The MSB must validate the client certificate against a pinned
  manufacturer CA; an edge proxy would strip the client certificate or terminate mTLS
  before the application sees it.
- **Decision:** mTLS terminates inside the application (Rust server, tokio-rustls). In
  k8s, use TLSRoute / nginx `ssl-passthrough` so the pod receives the client certificate.
  Terminating mTLS at an edge proxy is a hard-rule violation.
- **Consequences:** Production manifests use ssl-passthrough/TLSRoute for the MSB
  endpoint; the well-known endpoint uses a normal Ingress (public read, WebPKI).