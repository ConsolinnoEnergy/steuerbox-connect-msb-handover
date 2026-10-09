"""mTLS-only is a hard rule (AGENTS.md §6, ADR-0009): a request WITHOUT a client
certificate must never produce a decidable API answer — it is a transport failure
(client-side `unknown`), never `found:false`."""


def test_no_client_cert_is_transport_error(target, serial):
    import requests
    import urllib3

    urllib3.disable_warnings()
    try:
        resp = requests.get(f"{target}/v1/serials/{serial}", verify=False, timeout=10)
    except requests.exceptions.SSLError:
        return  # correct: TLS handshake without client cert is rejected
    # some TLS stacks accept then 40x at HTTP level — that is a violation only if the
    # answer is decidable discovery content
    assert resp.status_code not in (200, 404), (
        f"mTLS bypassed: got HTTP {resp.status_code} without a client certificate"
    )