#!/usr/bin/env python3
"""Migrate rs-retry tests/benches from Retry.sync() API to RetryConfig + executors."""

from __future__ import annotations

import re
import sys
from pathlib import Path


def add_imports(text: str) -> str:
    needs_config = "RetryConfig" in text
    needs_tokio = "TokioRetry::" in text
    needs_worker = "WorkerRetry::" in text

    def ensure(line_needle: str, import_line: str) -> None:
        nonlocal text
        if line_needle in text and import_line not in text:
            if "use qubit_retry::Retry;" in text:
                text = text.replace("use qubit_retry::Retry;", f"use qubit_retry::Retry;\n{import_line}")
            elif "use qubit_retry::{" in text and line_needle.split("::")[1] not in text.split("use qubit_retry::{")[1].split("}")[0]:
                text = text.replace("use qubit_retry::{", f"use qubit_retry::{{{line_needle.split('::')[1]}, ", 1)
            else:
                # first use line
                for i, line in enumerate(text.splitlines(True)):
                    if line.startswith("use qubit_retry::"):
                        insert = import_line + "\n"
                        parts = text.splitlines(True)
                        parts.insert(i + 1, insert)
                        text = "".join(parts)
                        break

    if needs_config:
        ensure("RetryConfig", "use qubit_retry::RetryConfig;")
    if needs_tokio:
        ensure("TokioRetry", "use qubit_retry::TokioRetry;")
    if needs_worker:
        ensure("WorkerRetry", "use qubit_retry::WorkerRetry;")
    return text


def replace_builders(text: str) -> str:
    # Retry::<T>::builder(retry_once_policy())
    text = re.sub(
        r"Retry::<([^>]+)>::builder\(\s*retry_once_policy\(\)\s*\)",
        r"RetryConfig::<\1>::builder().policy(retry_once_policy())",
        text,
    )
    # Retry::<T>::builder(expr) — single-line
    text = re.sub(
        r"Retry::<([^>]+)>::builder\(([^;\n]+)\)",
        r"RetryConfig::<\1>::builder().policy(\2)",
        text,
    )
    text = re.sub(
        r"Retry::builder\(",
        r"RetryConfig::builder().policy(",
        text,
    )
    return text


def unwrap_config_build(text: str) -> str:
    """Ensure RetryConfig builder chains end with .build().expect(...)."""

    def repl(match: re.Match[str]) -> str:
        chain = match.group(0)
        if ".build().expect" in chain or ".build().unwrap" in chain or ".build()?" in chain:
            return chain
        return chain[:- len(".build()")] + '.build().expect("valid config")'

    # Match RetryConfig... up to .build() not already unwrapped
    pattern = re.compile(
        r"RetryConfig(?:::<[^>]+>)?::builder\(\)(?:[^\n;]|\.policy\([^\)]*(?:\([^\)]*\)[^\)]*)*\))*\.build\(\)",
    )
    return pattern.sub(repl, text)


def facade_for_chain(chain: str) -> str:
    if "AttemptCancellationToken" in chain or "worker_stack_size" in chain or "worker_thread_name" in chain:
        return "WorkerRetry"
    if "async {" in chain or ".await" in chain:
        return "TokioRetry"
    if "hard_attempt_timeout" in chain or "hard_flow_timeout" in chain:
        if "AttemptCancellationToken" in chain:
            return "WorkerRetry"
        return "TokioRetry"
    return "Retry"


def replace_facade_calls(text: str) -> str:
    # inline: retry.sync().method... -> Retry::new(&retry).method...
    text = re.sub(r"(\w+)\.sync\(\)\.", r"Retry::new(&\1).", text)
    text = re.sub(r"(\w+)\.tokio\(\)\.", r"TokioRetry::new(&\1).", text)
    text = re.sub(r"(\w+)\.worker\(\)\.", r"WorkerRetry::new(&\1).", text)

    # standalone facade without following dot: retry.sync() -> Retry::new(&retry)
    text = re.sub(r"(\w+)\.sync\(\)", r"Retry::new(&\1)", text)
    text = re.sub(r"(\w+)\.tokio\(\)", r"TokioRetry::new(&\1)", text)
    text = re.sub(r"(\w+)\.worker\(\)", r"WorkerRetry::new(&\1)", text)

    # multiline orphaned .sync() / .tokio() / .worker()
    text = re.sub(r"\n(\s*)\.(sync|tokio|worker)\(\)\s*\n", r"\n", text)

    return text


def fix_helper_return_types(text: str) -> str:
    text = re.sub(r"-> Retry<", "-> RetryConfig<", text)
    text = re.sub(r"\(Retry<", "(RetryConfig<", text)
    return text


def fix_policy_build_corruption(text: str) -> str:
    text = text.replace('\\"valid policy\\"', '"valid policy"')
    return re.sub(
        r'(RetryPolicy::builder\(\)(?:[^\n]|\.[\w]+\([^\)]*\))*?)\.build\(\)\.expect\("valid config"\)',
        r'\1.build().expect("valid policy")',
        text,
    )


def unwrap_retry_config_build(text: str) -> str:
    lines = text.splitlines(True)
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if re.search(r"RetryConfig(?:::<[^>]+>)?::builder\(\)", line):
            block = [line]
            if re.search(r"\.build\(\)", line) and ".expect(" not in line and ".unwrap(" not in line:
                block[0] = line.replace(".build()", '.build().expect("valid config")', 1)
                out.extend(block)
                i += 1
                continue
            j = i + 1
            while j < len(lines):
                block.append(lines[j])
                if re.search(r"\.build\(\)", lines[j]):
                    last = block[-1]
                    if ".expect(" not in last and ".unwrap(" not in last and "?" not in last.strip():
                        block[-1] = last.replace(".build()", '.build().expect("valid config")', 1)
                    out.extend(block)
                    i = j + 1
                    break
                j += 1
            else:
                out.append(line)
                i += 1
        else:
            out.append(line)
            i += 1
    return "".join(out)


def fix_multiline_run(text: str) -> str:
    """Repair chains where `.sync()` was removed but `.run` remained on config."""
    text = re.sub(
        r"(\w+)\s*\n(\s*)\.run\(",
        lambda m: f"Retry::new(&{m.group(1)})\n{m.group(2)}.run("
        if m.group(1) not in ("Retry", "TokioRetry", "WorkerRetry")
        else m.group(0),
        text,
    )
    return text


def fix_run_chained_on_builder(text: str) -> str:
    """Repair `.run()` left on the builder after `.worker()`/`.sync()` removal."""

    def repl(match: re.Match[str]) -> str:
        chain = match.group(1)
        run_and_rest = match.group(2)
        if ".build()" in chain:
            return match.group(0)
        executor = "WorkerRetry" if "AttemptCancellationToken" in run_and_rest else "Retry"
        if "async {" in run_and_rest or ".await" in run_and_rest:
            executor = "TokioRetry"
        return (
            f"let config = {chain}.build().expect(\"valid config\");\n"
            f"    {executor}::new(&config){run_and_rest}"
        )

    pattern = re.compile(
        r"((?:let \w+ = )?RetryConfig(?:::<[^>]+>)?::builder\(\)(?:[\s\S]*?))(\.run\([\s\S]*?\))\s*(?=\.expect_err|\.unwrap_err|\.await|;|\n\s*assert)",
    )
    return pattern.sub(repl, text)


def fix_timer_on_builder(text: str) -> str:
    pattern = re.compile(
        r"((?:let \w+ = )?RetryConfig(?:::<[^>]+>)?::builder\(\)(?:[\s\S]*?))(\.(?:hard_attempt_timeout|hard_flow_timeout|cancellation_token|worker_stack_size|timer)\([\s\S]*?\))\s*(?=\.run|\.expect_err|;) ",
    )
    def repl(match: re.Match[str]) -> str:
        chain = match.group(1)
        settings = match.group(2)
        if ".build()" in chain:
            return match.group(0)
        executor = "WorkerRetry" if "worker_stack_size" in settings or "AttemptCancellationToken" in text[match.start() : match.end() + 200] else "TokioRetry"
        if "cancellation_token" in settings and "AttemptCancellationToken" not in settings:
            executor = "Retry"
        return (
            f"let config = {chain}.build().expect(\"valid config\");\n"
            f"    {executor}::new(&config){settings}"
        )
    return pattern.sub(repl, text)


def migrate(text: str) -> str:
    text = text.replace("RetryBuilder", "RetryConfigBuilder")
    text = text.replace("SyncRetry", "Retry")
    text = replace_builders(text)
    text = replace_facade_calls(text)
    text = fix_multiline_run(text)
    text = fix_run_chained_on_builder(text)
    text = fix_timer_on_builder(text)
    text = fix_helper_return_types(text)
    text = add_imports(text)
    return text


def should_migrate(text: str) -> bool:
    return any(
        needle in text
        for needle in (
            "Retry::builder",
            "Retry::<",
            ".sync()",
            ".tokio()",
            ".worker()",
            "SyncRetry",
            "RetryBuilder",
        )
    )


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    paths = list((root / "tests").rglob("*.rs")) + [root / "benches" / "retry_benchmarks.rs"]
    for path in paths:
        if "target" in path.parts:
            continue
        original = path.read_text()
        if not should_migrate(original):
            continue
        updated = migrate(original)
        updated = fix_policy_build_corruption(updated)
        updated = unwrap_retry_config_build(updated)
        if updated != original:
            path.write_text(updated)
            print(f"updated {path.relative_to(root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
