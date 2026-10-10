"""Fleet binding (ADR-0018): the IANA PEN inside pairing_dataset.ship_id must match
the iana_pen the target pinned from the manufacturer's well-known document.
Foreign-fleet payloads → 422 fleet_mismatch + operator alarm, never a silent ignore.
PEN under test comes from CONF_IANA_PEN (default 12345, the example PEN)."""

from conftest import error_code, pairing_dataset


def _provision(client, target, serial, ship_id, iana_pen):
    body = {"pairing_dataset": pairing_dataset(ship_id, iana_pen=iana_pen)}
    return client.post(
        f"{target}/v1/serials/{serial}/provisioning", json=body, timeout=10
    )


def test_foreign_iana_pen_is_422_fleet_mismatch(client, target, serial, iana_pen):
    # a PEN the MSB did not pin — must never be silently accepted as own-fleet data
    r = _provision(client, target, serial, "i:99999_u:ship-foreign-fleet", iana_pen)
    assert r.status_code == 422, f"foreign PEN must 422, got {r.status_code}: {r.text[:200]}"
    assert error_code(r) == "fleet_mismatch", error_code(r)


def test_ship_id_without_iana_pen_is_422_fleet_mismatch(client, target, serial, iana_pen):
    # ship_id outside the SHIP-ID shape (no i:<PEN>_u: part) → fleet_mismatch too
    r = _provision(client, target, serial, "ship-without-pen", iana_pen)
    assert r.status_code == 422, f"missing PEN must 422, got {r.status_code}: {r.text[:200]}"
    assert error_code(r) == "fleet_mismatch", error_code(r)


def test_own_iana_pen_is_accepted(client, target, serial, iana_pen):
    # uses the SAME shared ship_id as the rest of the suite (write-once invariant):
    # if another test already provisioned it, this is idempotent 202 — still proof
    # the own-fleet PEN is accepted
    r = _provision(client, target, serial, f"i:{iana_pen}_u:ship-conformance", iana_pen)
    assert r.status_code == 202, f"own-fleet PEN must 202, got {r.status_code}: {r.text[:200]}"
    assert r.json().get("status") == "accepted"
