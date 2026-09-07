#!/usr/bin/env python3
"""Execute annotated Rust and Cargo examples from both README/guide languages.

Each Rust block is a complete binary. Cargo blocks are dependency snippets and
are checked with an empty binary. An immediately preceding annotation declares
kind=run|cargo and features=none|serde|tokio|serde,tokio. Undeclared blocks fail
closed. Examples run from a fresh temporary directory, using this checkout via
a path dependency. Clock test utilities and serde_json are available as explicit
example fixtures, never as public re-exports from qubit-retry.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

ANNOTATION = re.compile(r"<!-- retry-example: kind=(run|cargo) features=([a-z,]+) -->")
DOCUMENTS = ("README.md", "README.zh_CN.md", "doc/user_guide.md", "doc/user_guide.zh_CN.md")


def extract(path):
    """Return executable blocks with their original source locations."""
    lines = path.read_text(encoding="utf-8").splitlines()
    result = []
    index = 0
    while index < len(lines):
        language = lines[index].strip().removeprefix("```")
        if lines[index].strip().startswith("```") and language.startswith(("rust", "toml")) and language not in ("rust", "toml"):
            raise ValueError(f"{path}:{index + 1}: unsupported executable fence modifier")
        if not lines[index].strip().startswith("```") or language not in ("rust", "toml"):
            index += 1
            continue
        start = index
        annotation = ANNOTATION.fullmatch(lines[index - 1].strip()) if index else None
        if annotation is None:
            raise ValueError(f"{path}:{start + 1}: missing executable example annotation")
        kind, feature_text = annotation.groups()
        if (language == "rust") != (kind == "run"):
            raise ValueError(f"{path}:{start + 1}: example kind disagrees with fence language")
        features = [] if feature_text == "none" else feature_text.split(",")
        if any(feature not in ("serde", "tokio") for feature in features) or len(set(features)) != len(features):
            raise ValueError(f"{path}:{start + 1}: unknown feature or repeated feature")
        index += 1
        body = []
        while index < len(lines) and lines[index].strip() != "```":
            body.append(lines[index])
            index += 1
        if index == len(lines):
            raise ValueError(f"{path}:{start + 1}: unclosed code block")
        result.append(dict(path=path, line=start + 2, kind=kind, features=features, code="\n".join(body) + "\n"))
        index += 1
    return result


def cargo_dependency(example, version):
    """Validate the advertised dependency, including its declared feature set."""
    data = tomllib.loads(example["code"])
    dependency = data.get("dependencies", {}).get("qubit-retry")
    if isinstance(dependency, str):
        dependency = {"version": dependency}
    if not isinstance(dependency, dict):
        raise ValueError("Cargo example requires [dependencies].qubit-retry")
    expected = {version, ".".join(version.split(".")[:2])}
    if dependency.get("version") not in expected:
        raise ValueError(f"Cargo example version must describe {version}")
    if set(dependency.get("features", [])) != set(example["features"]):
        raise ValueError("Cargo example features disagree with annotation")
    if set(data) != {"dependencies"} or set(data["dependencies"]) != {"qubit-retry"}:
        raise ValueError("Cargo example must contain only the documented qubit-retry dependency")
    return dependency


def run_examples(root):
    """Compile and run every language variant without relying on repository cwd."""
    if shutil.which("cargo") is None:
        raise ValueError("cargo is required to verify documentation examples")
    package = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    examples = [example for name in DOCUMENTS for example in extract(root / name)]
    if not examples:
        raise ValueError("no executable documentation examples found")
    with tempfile.TemporaryDirectory(prefix="rs-retry-doc-examples-") as directory:
        workspace = Path(directory)
        for ordinal, example in enumerate(examples):
            location = f'{example["path"].relative_to(root)}:{example["line"]}'
            try:
                if example["kind"] == "cargo":
                    cargo_dependency(example, package["package"]["version"])
                consumer = workspace / f"example-{ordinal}"
                (consumer / "src").mkdir(parents=True)
                manifest = (
                    '[package]\nname = "retry-doc-example"\nversion = "0.0.0"\nedition = "2024"\n'
                    '[dependencies]\nqubit-retry = { path = ' + json.dumps(str(root)) +
                    ', default-features = false, features = ' + json.dumps(example["features"]) + ' }\n'
                    'qubit-clock = { version = "0.13", features = ["test-util"] }\nserde_json = "1"\n'
                )
                if "tokio" in example["features"]:
                    manifest += 'tokio = { version = "1.52", features = ["rt", "macros", "time"] }\n'
                (consumer / "Cargo.toml").write_text(manifest, encoding="utf-8")
                # Reuse the verified dependency resolution, including locked yanked versions.
                # Cargo prunes unused development dependencies for each consumer.
                shutil.copyfile(root / "Cargo.lock", consumer / "Cargo.lock")
                code = example["code"] if example["kind"] == "run" else "fn main() {}\n"
                (consumer / "src/main.rs").write_text(code, encoding="utf-8")
                env = os.environ.copy()
                env["CARGO_TARGET_DIR"] = str(workspace / "target")
                completed = subprocess.run(
                    ["cargo", "run", "--offline", "--quiet"], cwd=consumer, env=env,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120,
                )
                if completed.returncode:
                    raise ValueError(completed.stdout)
            except (ValueError, subprocess.TimeoutExpired) as error:
                raise ValueError(f"{location}: documentation example failed\n{error}") from error
            print(f"PASS {location} ({example['kind']}, features={example['features']})", flush=True)
    print(f"Verified {len(examples)} documentation examples in both languages.")


def main():
    """Use this script's checkout, independent of the caller's working directory."""
    try:
        run_examples(Path(__file__).resolve().parents[1])
    except (ValueError, OSError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
