"""Error taxonomy (spec/provisioning.yaml ErrorResponse): every error body carries
the machine-readable code; the reference statuses are normative."""

from conftest import error_code


def _post(client, target, serial, payload=None, raw=None):
    url = f"{target}/v1/serials/{serial}/provisioning"
    if raw is not None:
        return client.post(
            url, data=raw, headers={"content-type": "application/json"}, timeout=10
        )
    return client.post(url, json=payload, timeout=10)


def test_not_canonical_serial_is_400(client, target, serial):
    from conftest import pairing_dataset

    r = _post(
        client,
        target,
        f"{serial}-X9",
        payload={"pairing_dataset": pairing_dataset()},
    )
    assert r.status_code == 400, f"non-canonical write must 400, got {r.status_code}"
    assert error_code(r) == "not_canonical", error_code(r)


def test_unknown_serial_is_404_not_found(client, target):
    body = {
        "pairing_dataset": {
            "ship_id": "ship-conformance",
            "ski": "s",
            "qr_info": "SHIP;SKI:conf-ski;ID:conf-id;ENDSHIP;",
        }
    }
    r = _post(client, target, "SNCONF9999", payload=body)
    assert r.status_code == 404, f"unknown serial must 404, got {r.status_code}"
    assert error_code(r) == "not_found", error_code(r)


def test_invalid_hems_serial_is_422_invalid_pairing_dataset(client, target, serial):
    body = {
        "pairing_dataset": {
            "ship_id": "ship-conformance",
            "ski": "s",
            "qr_info": "SHIP;SKI:x;",  # no ENDSHIP; → invalid dataset
        }
    }
    r = _post(client, target, serial, payload=body)
    assert r.status_code == 422, f"got {r.status_code}: {r.text[:200]}"
    assert error_code(r) == "invalid_pairing_dataset", error_code(r)


def test_invalid_body_is_422_invalid_body(client, target, serial):
    r = _post(client, target, serial, raw="not-json")
    assert r.status_code == 422, f"got {r.status_code}: {r.text[:200]}"
    assert error_code(r) == "invalid_body", error_code(r)