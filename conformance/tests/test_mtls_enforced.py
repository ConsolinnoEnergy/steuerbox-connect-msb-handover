"""mTLS-only is a hard rule (AGENTS.md §6, ADR-0009): a request WITHOUT a client
certificate must never produce a decidable API answer — it is a transport failure
(client-side `unknown`), never `found:false`."""


def test_no_client_cert_is_transport_error(client, target, serial):
    import requests

    # Establish that the target is reachable with a trusted server certificate
    # and valid client credentials before testing rejection of an anonymous client.
    url = f"{target}/v1/serials/{serial}"
    client.get(url, timeout=10).raise_for_status()
    try:
        resp = requests.get(url, verify=client.verify, timeout=10)
    except requests.exceptions.SSLError as exc:
        assert "CERTIFICATE_VERIFY_FAILED" not in str(exc), (
            f"server certificate verification failed, not client authentication: {exc}"
        )
        return  # correct: TLS handshake without client cert is rejected
    # some TLS stacks accept then 40x at HTTP level — that is a violation only if the
    # answer is decidable discovery content
    assert resp.status_code not in (200, 404), (
        f"mTLS bypassed: got HTTP {resp.status_code} without a client certificate"
    )
