"""Tri-state (ADR-0005): discovery is status-code based — 200 = found (empty
body), 404 = not found (empty body); the answer never transmits state bodies.
A dead target is never 404."""


def test_found_is_200_without_body(client, target, serial):
    r = client.get(f"{target}/v1/serials/{serial}", timeout=10)
    assert r.status_code == 200, f"discovery must be 200, got {r.status_code}"
    assert r.content == b"", "found must be an empty 200 (canonical is derived, not transmitted)"


def test_unknown_serial_is_404(client, target, serial):
    r = client.get(f"{target}/v1/serials/SNCONF9999", timeout=10)
    assert r.status_code == 404
    assert r.content == b"", "not-found must be an empty 404 (no existence leak)"


def test_unknown_state_is_never_transmitted(client, target, serial):
    r = client.get(f"{target}/v1/serials/{serial}", timeout=10)
    assert r.status_code in (200, 404), (
        "the server must only answer 200/404 for decidable queries (ADR-0005)"
    )


def test_dead_target_is_transport_failure_not_found_false():
    import requests

    try:
        r = requests.get("https://127.0.0.1:1/v1/serials/SNCONF9999", verify=False, timeout=3)
    except requests.exceptions.RequestException:
        return  # transport failure — client derives `unknown`; correct
    assert r.status_code != 404, (
        "a dead/unreachable MSB must never be concluded as `not found`"
    )