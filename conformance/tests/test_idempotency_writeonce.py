"""Idempotency + write-once (ADR-0006): 202 on first write AND on idempotent replay;
a different ship_id on the same serial → 409 write_once_conflict."""


def _provision(client, target, serial, ship_id):
    from conftest import pairing_dataset

    body = {"pairing_dataset": pairing_dataset(ship_id)}
    return client.post(
        f"{target}/v1/serials/{serial}/provisioning", json=body, timeout=10
    )


def test_idempotent_and_write_once(client, target, serial):
    r1 = _provision(client, target, serial, "ship-conformance")
    assert r1.status_code == 202, f"first provision must 202, got {r1.status_code}: {r1.text[:200]}"
    assert r1.json().get("status") == "accepted"

    r2 = _provision(client, target, serial, "ship-conformance")
    assert r2.status_code == 202, "idempotent replay must be 202 as well"
    assert r2.json().get("status") == "accepted"

    r3 = _provision(client, target, serial, "ship-conformance-OTHER")
    assert r3.status_code == 409, f"write-once violation must 409, got {r3.status_code}"
    from conftest import error_code

    assert error_code(r3) == "write_once_conflict", error_code(r3)