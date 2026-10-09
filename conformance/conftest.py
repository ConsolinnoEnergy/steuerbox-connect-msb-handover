"""Conftest: black-box conformance suite — target MSB comes from --target / TARGET.

Credentials for the mTLS API (server must have this client CA pinned):
- CONF_CERT / CONF_KEY  — mTLS client certificate + key (PEM)
- CONF_ROOT             — server-TLS root bundle to verify the target against
- CONF_SERIAL           — a serial in the target's portfolio (default SNTEST0001)
- CONF_RATE_TARGET      — optional base URL of a rate-limited instance (default: none;
  rate-limit cases are skipped without it)
"""
import os

import pytest
import requests


def pytest_addoption(parser):
    parser.addoption("--target", default=os.environ.get("TARGET", "https://localhost:8443"))


@pytest.fixture(scope="session")
def target(request) -> str:
    return request.config.getoption("--target").rstrip("/")


@pytest.fixture(scope="session")
def client(target) -> requests.Session:
    s = requests.Session()
    cert, key = os.environ.get("CONF_CERT"), os.environ.get("CONF_KEY")
    if cert and key:
        s.cert = (cert, key)
    root = os.environ.get("CONF_ROOT")
    if root:
        s.verify = root
    else:
        # self-signed test targets: accept, mTLS is what we test
        s.verify = False
    return s


@pytest.fixture(scope="session")
def serial() -> str:
    return os.environ.get("CONF_SERIAL", "SNTEST0001")


@pytest.fixture(scope="session")
def rate_target() -> str | None:
    return os.environ.get("CONF_RATE_TARGET")


# shared valid provisioning body; ship_id is conformance-unique so write-once tests
# can derive a conflict deterministically
def pairing_dataset(ship_id: str = "ship-conformance") -> dict:
    return {
        "ship_id": ship_id,
        "ski": "SNTESTCONF_SKI0001",
        "qr_info": "SHIP;SKI:conf-ski;ID:conf-hems-id;FPH256:conf-fph-fingerprint;ENDSHIP;",
    }


def error_code(resp: requests.Response) -> str | None:
    try:
        return resp.json().get("error")
    except Exception:  # noqa: BLE001 — non-JSON body
        return None