#!/usr/bin/env python3
"""Check that worker-only public APIs are gated by the worker feature."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "worker_disabled"
EXPECTED_IMPORTS = {
    "AttemptCancellationToken": "use qubit_retry::AttemptCancellationToken;",
    "WorkerRetry": "use qubit_retry::WorkerRetry;",
}


class GateCheckError(RuntimeError):
    """A feature-gate check failed with stage-specific context."""


def parse_cargo_messages(stdout: str) -> list[dict]:
    messages = []
    for line_number, line in enumerate(stdout.splitlines(), 1):
        if not line.strip():
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            raise GateCheckError(f"Cargo emitted non-JSON stdout at line {line_number}: {line!r}") from error
        if not isinstance(message, dict):
            raise GateCheckError(f"Cargo emitted a non-object JSON value at line {line_number}")
        messages.append(message)
    return messages


def _error_diagnostics(messages: list[dict]) -> list[dict]:
    return [
        item["message"]
        for item in messages
        if item.get("reason") == "compiler-message"
        and isinstance(item.get("message"), dict)
        and item["message"].get("level") == "error"
    ]


def _diagnostic_imports(diagnostic: dict, expected_source: Path, cwd: Path) -> set[str]:
    code = (diagnostic.get("code") or {}).get("code")
    if code not in {"E0432", "E0433"}:
        return set()
    found = set()
    for span in diagnostic.get("spans", []):
        file_name = span.get("file_name")
        if not file_name:
            continue
        span_path = Path(file_name)
        if not span_path.is_absolute():
            span_path = cwd / span_path
        source_path = expected_source if expected_source.is_absolute() else cwd / expected_source
        if span_path.resolve() != source_path.resolve():
            continue
        for line in span.get("text", []):
            content = line.get("text", "").strip()
            for symbol, import_line in EXPECTED_IMPORTS.items():
                if content == import_line:
                    found.add(symbol)
    return found


def evaluate_result(
    returncode: int,
    stdout: str,
    stderr: str,
    *,
    enabled: bool,
    stage: str,
    source_path: Path = Path("consumer/src/main.rs"),
    cwd: Path = Path("."),
) -> None:
    """Validate a Cargo JSON result; only the two gated imports may fail disabled."""
    details = f"stage={stage}, exit={returncode}"
    if stderr.strip():
        details += f"\nstderr:\n{stderr.strip()}"
    try:
        messages = parse_cargo_messages(stdout)
    except GateCheckError as error:
        raise GateCheckError(f"{details}\n{error}") from error
    diagnostics = _error_diagnostics(messages)
    if enabled:
        if returncode != 0 or diagnostics:
            raise GateCheckError(f"worker-enabled cargo check failed; {details}\n{stdout.strip()}")
        return

    if returncode == 0:
        raise GateCheckError(f"worker-disabled cargo check unexpectedly succeeded; {details}")
    if not diagnostics:
        raise GateCheckError(f"worker-disabled cargo check failed without compiler diagnostics; {details}\n{stdout.strip()}")
    found: set[str] = set()
    unexpected = []
    for diagnostic in diagnostics:
        symbols = _diagnostic_imports(diagnostic, source_path, cwd)
        if not symbols:
            unexpected.append(diagnostic)
        else:
            found.update(symbols)
    missing = set(EXPECTED_IMPORTS) - found
    if unexpected or missing:
        raise GateCheckError(
            f"worker-disabled diagnostics did not match the feature gate; {details}; "
            f"missing={sorted(missing)}; unexpected={json.dumps(unexpected, ensure_ascii=False)}"
        )


def prepare_consumer(directory: Path) -> Path:
    """Copy the fixture into a temporary consumer with an absolute crate path."""
    source = FIXTURE / "src" / "main.rs"
    manifest = FIXTURE / "Cargo.toml"
    source_text = source.read_text(encoding="utf-8")
    manifest_text = manifest.read_text(encoding="utf-8")
    path_line = re.compile(r'(?m)^(\s*qubit-retry\s*=\s*\{[^\n]*?path\s*=\s*)"[^"]+"')
    manifest_text, replacements = path_line.subn(
        lambda match: match.group(1) + '"' + str(ROOT) + '"', manifest_text
    )
    if replacements != 1:
        raise GateCheckError(f"fixture manifest must contain exactly one qubit-retry path dependency; found {replacements}")
    consumer = directory / "consumer"
    (consumer / "src").mkdir(parents=True)
    (consumer / "Cargo.toml").write_text(manifest_text, encoding="utf-8")
    (consumer / "src" / "main.rs").write_text(source_text, encoding="utf-8")
    lockfile = ROOT / "Cargo.lock"
    if lockfile.exists():
        shutil.copyfile(lockfile, consumer / "Cargo.lock")
    return consumer


def run_cargo(command: list[str], *, cwd: Path, target: Path, stage: str, runner=subprocess.run):
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    env["CARGO_TERM_COLOR"] = "never"
    env["LC_ALL"] = "C"
    env["LANG"] = "C"
    env["RUSTC_WRAPPER"] = ""
    try:
        result = runner(
            command,
            cwd=cwd,
            env=env,
            text=True,
            capture_output=True,
            timeout=120,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise GateCheckError(f"{stage}: cargo command timed out after 120 seconds") from error
    except OSError as error:
        raise GateCheckError(f"{stage}: could not execute cargo: {error}") from error
    return result


def check(*, fetch: bool = False, cargo="cargo", temp_root=None, runner=subprocess.run) -> None:
    with tempfile.TemporaryDirectory(prefix="worker feature gate ", dir=temp_root) as temp_name:
        temporary = Path(temp_name)
        consumer = prepare_consumer(temporary)
        target = temporary / "target"
        common = [cargo, "check", "--manifest-path", str(consumer / "Cargo.toml"), "--offline", "--no-default-features"]
        if fetch:
            fetch_command = [cargo, "fetch", "--manifest-path", str(consumer / "Cargo.toml")]
            fetched = run_cargo(fetch_command, cwd=consumer, target=target, stage="dependency fetch", runner=runner)
            if fetched.returncode != 0:
                raise GateCheckError(f"dependency fetch failed with exit={fetched.returncode}\n{fetched.stdout}\n{fetched.stderr}")
        for enabled in (True, False):
            stage = "worker enabled" if enabled else "worker disabled"
            command = common.copy()
            if enabled:
                command.extend(["--features", "worker"])
            command.extend(["--message-format=json"])
            result = run_cargo(command, cwd=consumer, target=target, stage=stage, runner=runner)
            evaluate_result(
                result.returncode,
                result.stdout,
                result.stderr,
                enabled=enabled,
                stage=stage,
                source_path=consumer / "src" / "main.rs",
                cwd=consumer,
            )
            print(f"{stage}: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true", help="fetch dependencies before offline checks")
    args = parser.parse_args()
    try:
        check(fetch=args.fetch)
    except GateCheckError as error:
        print(f"worker feature gate check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
