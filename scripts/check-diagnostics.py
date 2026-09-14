#!/usr/bin/env python3
"""Check native proc-macro error messages and source ranges.

Requires Python 3 and Rust 1.95. CARGO_TARGET_DIR is respected.
"""

import json
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
CASES = {
    "deprecated_subject": ("use of deprecated field `Config::old`", "config.old"),
    "deprecated_object": ("use of deprecated field `ConfigOptional::old`", "config.old"),
    "attrs_selector": ("attribute selectors accept only a path", "("),
    "attrs_missing_path": ("expected identifier", ","),
    "attrs_separator": ("expected `,`", "derive"),
    "attrs_literal": ("Unexpected meta-item format `literal`", '"invalid"'),
    "attrs_mapping": ("`attrs` cannot be used when `object` or `subject` is specified", "attrs"),
    "attrs_generated": ("cannot find derive macro `Missing`", "Missing"),
    "cfg_field": ("`skip` can only be combined with `default`", "skip"),
    "cfg_attribute": ("`skip` can only be combined with `default`", "skip"),
    "default_flatten": ("`default` cannot be used with `flatten`", "default"),
    "default_mutable": ("mismatched types", "mutable_default"),
}


def main():
    build = subprocess.run(
        ["cargo", "+1.95", "build", "--offline", "-p", "optionize", "--message-format=json"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if build.returncode:
        raise RuntimeError(build.stderr + build.stdout)
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
    library = next(
        Path(filename)
        for artifact in artifacts
        if artifact.get("reason") == "compiler-artifact"
        and artifact["target"]["name"] == "optionize"
        for filename in artifact["filenames"]
        if filename.endswith(".rlib")
    )
    with tempfile.TemporaryDirectory(prefix="optionize-diagnostics-") as output:
        for name, (message, highlighted) in CASES.items():
            source = ROOT / "optionize" / "tests" / "ui" / f"{name}.rs"
            result = subprocess.run(
                [
                    "rustc", "+1.95", "--edition=2024", "--crate-type=lib",
                    "--emit=metadata", "--out-dir", output, "--error-format=json",
                    "--extern", f"patches={library}",
                    "-L", f"dependency={library.parent / 'deps'}", str(source),
                ],
                capture_output=True, text=True,
            )
            diagnostics = [json.loads(line) for line in result.stderr.splitlines() if line.startswith("{")]
            errors = [item for item in diagnostics if item.get("level") == "error" and item.get("spans")]
            assert result.returncode and len(errors) == 1, result.stderr
            assert message in errors[0]["message"], result.stderr
            spans = [span for span in errors[0]["spans"] if span["is_primary"]]
            assert len(spans) == 1, result.stderr
            span = spans[0]
            assert Path(span["file_name"]).resolve() == source, result.stderr
            text = source.read_bytes()[span["byte_start"]:span["byte_end"]].decode()
            assert text == highlighted, result.stderr
            print(f"{name}: message and source range match")


if __name__ == "__main__":
    main()
