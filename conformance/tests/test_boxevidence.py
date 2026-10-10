"""box_evidence (ADR-0007): mismatch → 422 box_mismatch; absent evidence → accepted;
a mismatching answer must not silently ignore the evidence.

Note on state: provisioning is write-once per (serial, ship_id) — this file uses the
SAME ship_id as the rest of the suite so tests stay order-independent."""


def _provision(client, target, serial, evidence=None, iana_pen="12345"):
    from conftest import pairing_dataset

    body = {"pairing_dataset": pairing_dataset(f"i:{iana_pen}_u:ship-conformance", iana_pen=iana_pen)}
    if evidence is not None:
        body["box_evidence"] = evidence
    return client.post(
        f"{target}/v1/serials/{serial}/provisioning", json=body, timeout=10
    )


def test_no_evidence_is_accepted(client, target, serial, iana_pen):
    r = _provision(client, target, serial, iana_pen=iana_pen)
    assert r.status_code == 202, f"server MUST accept without evidence, got {r.status_code}: {r.text[:200]}"


def test_mismatching_evidence_is_422_box_mismatch(client, target, serial, iana_pen):
    r = _provision(
        client, target, serial, evidence={"ship_id": "i:51593_u:WRONG", "ski": "WRONGSKI"}, iana_pen=iana_pen
    )
    assert r.status_code == 422, f"got {r.status_code}: {r.text[:200]}"
    assert r.json().get("error") == "box_mismatch"


def test_partial_evidence_is_422_box_mismatch(client, target, serial, iana_pen):
    r = _provision(client, target, serial, evidence={"ship_id": "i:51593_u:WRONG"}, iana_pen=iana_pen)
    assert r.status_code == 422, f"partial evidence counts as mismatch, got {r.status_code}"
    assert r.json().get("error") == "box_mismatch"