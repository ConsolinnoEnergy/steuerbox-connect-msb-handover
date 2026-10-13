"""Check tracked and publishable files for private integration data, without printing it."""

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RULES = {
    "private host": r"(?:[\w.-]+\.)?consolinno-it\.de",
    "private integration": r"ems-provisioner|HEMS_BACKEND_|HEMS_QR_KEY|mender|leaflet",
    "non-synthetic device example": r"1[uU]0022[_-][Cc][Oo]-4206|GTHE0100045088",
    "private key": r"-----BEGIN (?:RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----",
    "private LAN address": r"\b(?:10\.\d+\.\d+\.\d+|192\.168\.\d+\.\d+|172\.(?:1[6-9]|2\d|3[01])\.\d+\.\d+)\b",
}


def main() -> int:
    output = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT
    )
    failures = []
    for name in sorted(set(output.decode().split("\0")) - {""}):
        path = ROOT / name
        if not path.is_file():
            continue
        if name.startswith("test/ca/out/") or path.name == ".env" or path.name.endswith(
            ("-key.pem", ".key", ".p12", ".pfx")
        ):
            failures.append(f"{name}: generated credential file")
        # This file defines the detection signatures, not application data.
        if path == Path(__file__).resolve():
            continue
        text = path.read_bytes().decode("utf-8", errors="replace")
        for number, line in enumerate(text.splitlines(), 1):
            for rule, pattern in RULES.items():
                if re.search(pattern, line, re.IGNORECASE):
                    failures.append(f"{name}:{number}: {rule}")
    for failure in failures:
        print(failure)
    print(f"public-check: {len(failures)} findings")
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
