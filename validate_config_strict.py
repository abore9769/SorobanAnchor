#!/usr/bin/env python3
"""Strict JSON-Schema validator for AnchorKit config files (JSON and TOML)."""
import sys
import json
import pathlib

def load_config(path: pathlib.Path) -> dict:
    if path.suffix == ".toml":
        try:
            import tomllib  # Python 3.11+
        except ImportError:
            try:
                import tomli as tomllib  # pip install tomli
            except ImportError:
                import toml as tomllib   # pip install toml (legacy)
                return tomllib.loads(path.read_text(encoding="utf-8"))
        return tomllib.loads(path.read_bytes())
    return json.loads(path.read_text(encoding="utf-8"))

def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <config_file> <schema_file>", file=sys.stderr)
        sys.exit(2)

    config_path = pathlib.Path(sys.argv[1])
    schema_path = pathlib.Path(sys.argv[2])

    try:
        import jsonschema
    except ImportError:
        print("ERROR: jsonschema not installed. Run: pip install jsonschema", file=sys.stderr)
        sys.exit(2)

    try:
        config = load_config(config_path)
    except Exception as e:
        print(f"ERROR: Failed to parse {config_path.name}: {e}", file=sys.stderr)
        sys.exit(1)

    schema = json.loads(schema_path.read_text(encoding="utf-8"))

    validator = jsonschema.Draft7Validator(schema)
    errors = sorted(validator.iter_errors(config), key=lambda e: list(e.absolute_path))

    if errors:
        for err in errors:
            path = " -> ".join(str(p) for p in err.absolute_path) or "(root)"
            print(f"  [{path}] {err.message}", file=sys.stderr)
        print(f"\nFAIL: {config_path.name} — {len(errors)} error(s)", file=sys.stderr)
        sys.exit(1)

    print(f"OK: {config_path.name}")

if __name__ == "__main__":
    main()
