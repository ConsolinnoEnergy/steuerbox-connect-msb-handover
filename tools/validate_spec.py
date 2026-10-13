"""spec-validate: parse all spec files and validate examples against the JSON Schemas.

Checks (AGENTS.md §9):
1. Both OpenAPI files parse (YAML) and every external ./… $ref resolves to a file.
2. spec/examples/*.json parse as JSON.
3. pairing-dataset.json and the pairing_dataset member of provisioning-request.json
   validate against pairing_dataset.schema.json.
4. wellknown-v1.json validates against wellknown.schema.json.
"""

import json
import sys
from pathlib import Path

try:
    import yaml
except ModuleNotFoundError:  # pragma: no cover — CI installs via requirements
    yaml = None

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parent.parent
SPEC = ROOT / "spec"

errors: list[str] = []


def load_json(path: Path):
    try:
        return json.loads(path.read_text())
    except Exception as e:  # noqa: BLE001 — collect and continue
        errors.append(f"{path}: invalid JSON: {e}")
        return None


def load_yaml(path: Path):
    if yaml is None:
        errors.append("pyyaml not installed; cannot parse the OpenAPI files")
        return None
    try:
        return yaml.safe_load(path.read_text())
    except Exception as e:  # noqa: BLE001
        errors.append(f"{path}: invalid YAML: {e}")
        return None


def check_external_refs(doc, path: Path) -> None:
    """Every ./… $ref must point to an existing file next to the OpenAPI file."""
    def walk(node):
        if isinstance(node, dict):
            ref = node.get("$ref")
            if isinstance(ref, str) and ref.startswith("./") and not (path.parent / ref).exists():
                    errors.append(f"{path.name}: unresolved $ref {ref}")
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)

    walk(doc)


def validate(instance, schema, label: str) -> None:
    for e in Draft202012Validator(schema).iter_errors(instance):
        errors.append(f"{label}: schema violation: {e.message}")


def main() -> int:
    for name in ("discovery.yaml", "provisioning.yaml", "wellknown.yaml"):
        doc = load_yaml(SPEC / name)
        if doc is not None:
            check_external_refs(doc, SPEC / name)
            if name == "discovery.yaml":
                query_schema = doc["paths"]["/v1/serials/{serial}"]["get"]["parameters"][0]["schema"]
                for query in ("S", "i:51593_u:SNTEST0001-" + "X" * 64):
                    validate(query, query_schema, "discovery query (short prefix / full suffixed VPID)")

    pairing_schema = load_json(SPEC / "pairing_dataset.schema.json")
    wellknown_schema = load_json(SPEC / "wellknown.schema.json")
    if pairing_schema is not None:
        pairing_schema = {
            k: v for k, v in pairing_schema.items() if k not in ("$schema", "$id")
        }
    if wellknown_schema is not None:
        wellknown_schema = {
            k: v for k, v in wellknown_schema.items() if k not in ("$schema", "$id")
        }

    examples = {f.name: load_json(f) for f in sorted((SPEC / "examples").glob("*.json"))}

    if pairing_schema is not None:
        ds = examples.get("pairing-dataset.json")
        if ds is not None:
            validate(ds, pairing_schema, "examples/pairing-dataset.json")
        request = examples.get("provisioning-request.json")
        if request is not None:
            if not isinstance(request.get("pairing_dataset"), dict):
                errors.append("examples/provisioning-request.json: missing pairing_dataset")
            else:
                validate(
                    request["pairing_dataset"],
                    pairing_schema,
                    "examples/provisioning-request.json (pairing_dataset)",
                )
    if wellknown_schema is not None:
        wk = examples.get("wellknown-v1.json")
        if wk is not None:
            validate(wk, wellknown_schema, "examples/wellknown-v1.json")
            # contentEncoding is an annotation in JSON Schema 2020-12 — assert it here:
            # the published client_ca.pem must actually be decodable base64 of a PEM.
            import base64 as _b64

            pem_b64 = wk.get("client_ca", {}).get("pem", "")
            try:
                decoded = _b64.b64decode(pem_b64, validate=True).decode()
                if "-----BEGIN CERTIFICATE-----" not in decoded:
                    errors.append("examples/wellknown-v1.json: decoded client_ca.pem is not a PEM certificate")
            except Exception as e:  # noqa: BLE001
                errors.append(f"examples/wellknown-v1.json: client_ca.pem is not valid base64: {e}")

    if errors:
        print("spec-validate FAILED:")
        for e in errors:
            print(f"  - {e}")
        return 1
    print("spec-validate OK: OpenAPI files parse, $refs resolve, examples validate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
