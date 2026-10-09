"""Rate limits are contract (ADR-0010): 429 + truthful Retry-After on discovery AND
provisioning; the body carries `rate_limited`.

Skipped unless CONF_RATE_TARGET points at a rate-limited instance (the default
reference quota is deliberately generous)."""
import time

import pytest
from conftest import error_code, pairing_dataset


def test_discovery_429_carries_retry_after_and_code(client, rate_target):
    if rate_target is None:
        pytest.skip("CONF_RATE_TARGET not set")
    target = rate_target.rstrip("/")
    for _ in range(10_000):
        r = client.get(f"{target}/v1/serials/SNCONF9999", timeout=10)
        if r.status_code == 429:
            assert error_code(r) == "rate_limited"
            retry = r.headers.get("Retry-After")
            assert retry is not None, "429 without Retry-After is a contract violation"
            assert retry.isdigit() and int(retry) >= 1, (
                f"Retry-After must be numeric seconds, got {retry!r}"
            )
            return
    assert False, "quota never exhausted after 10_000 requests"


def test_provisioning_honors_retry_after(client, rate_target):
    if rate_target is None:
        pytest.skip("CONF_RATE_TARGET not set")
    body = {"pairing_dataset": pairing_dataset("ship-conformance-rate")}
    for _ in range(10_000):
        r = client.post(
            f"{rate_target.rstrip('/')}/v1/serials/SNCONF9999/provisioning",
            json=body,
            timeout=10,
        )
        if r.status_code == 429:
            retry = r.headers.get("Retry-After", "0")
            assert retry.isdigit() and int(retry) >= 1, (
                "429 without Retry-After is a contract violation"
            )
            time.sleep(int(retry))  # honor Retry-After
            r2 = client.post(
                f"{rate_target.rstrip('/')}/v1/serials/SNCONF9999/provisioning",
                json=body,
                timeout=10,
            )
            assert r2.status_code == 404, (
                "after the window the request must pass rate limiting "
                "(404 = unknown serial here)"
            )
            return
    assert False, "provisioning quota never exhausted"