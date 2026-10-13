"""Generate the documentation's combined OpenAPI view from the normative spec files."""

import argparse
import json
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "docs/api/openapi.json"


def bundle() -> dict:
    result = {
        "openapi": "3.1.0",
        "info": {
            "title": "steuerbox-connect API",
            "version": "0.4.0",
            "description": "Generated from spec/*.yaml and their JSON schemas. Do not edit this view independently.",
        },
        "paths": {},
        "components": {"schemas": {}, "securitySchemes": {}},
    }
    for name in ("discovery", "provisioning", "wellknown"):
        path = ROOT / "spec" / f"{name}.yaml"
        doc = yaml.safe_load(path.read_text())

        def resolve(node, path=path, name=name):
            if isinstance(node, list):
                return [resolve(value) for value in node]
            if not isinstance(node, dict):
                return node
            ref = node.get("$ref", "")
            if ref.startswith("./"):
                data = json.loads((path.parent / ref).read_text())
                return resolve({**data, **{k: v for k, v in node.items() if k != "$ref"}})
            output = {key: resolve(value) for key, value in node.items()}
            if ref.startswith("#/components/schemas/"):
                output["$ref"] = f"#/components/schemas/{name}_{ref.rsplit('/', 1)[1]}"
            external = output.pop("externalValue", None)
            if external is not None:
                output["value"] = json.loads((path.parent / external).read_text())
            return output

        result["paths"].update(resolve(doc["paths"]))
        for schema, value in doc.get("components", {}).get("schemas", {}).items():
            result["components"]["schemas"][f"{name}_{schema}"] = resolve(value)
        result["components"]["securitySchemes"].update(
            doc.get("components", {}).get("securitySchemes", {})
        )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = json.dumps(bundle(), indent=2, ensure_ascii=False) + "\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != expected:
            print("docs/api/openapi.json is stale; run make spec-docs")
            return 1
        print("Generated OpenAPI view matches the normative sources")
    else:
        OUTPUT.write_text(expected)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
