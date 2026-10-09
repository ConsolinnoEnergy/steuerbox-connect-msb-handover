# ADR-0001: Two-CA Model — separate Server-TLS and Client-mTLS trust domains

- **Status:** Accepted
- **Context:** The MSB API needs a TLS server identity (public, WebPKI) and mTLS client
  authentication by manufacturer backends. Coupling the two lifecycles couples public
  availability to manufacturer CA policy and vice versa.
- **Decision:** Two independent CA domains. Server-TLS uses Let's Encrypt in k8s and a
  sim-LE root locally. Client-mTLS uses the manufacturer's dedicated sub-CA, published
  in the well-known document and pinned by the MSB (TOFU + TTL + rotation grace).
  The application never branches on the issuing CA; environment differences live in
  config and manifests only (`TLS_ROOT_BUNDLE`, cert paths, `IssuerRef`).
- **Consequences:** Rotation of either domain is autonomous. Local Compose reproduces
  production TLS semantics byte-identically. Mixing the domains is a hard-rule violation.