"""Unit tests for the worker feature-gate runner's Cargo JSON decisions."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock


SPEC = importlib.util.spec_from_file_location(
    "check_worker_feature_gate", Path(__file__).with_name("check_worker_feature_gate.py")
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def compiler_error(code, import_line=None, message="unresolved import"):
    spans = []
    if import_line:
        spans.append({"file_name": "consumer/src/main.rs", "text": [{"text": import_line}]})
    return {
        "reason": "compiler-message",
        "message": {
            "level": "error",
            "code": {"code": code} if code else None,
            "message": message,
            "spans": spans,
        },
    }


def cargo_json(*items):
    return "\n".join(json.dumps(item) for item in items) + "\n"


class CargoDiagnosticTests(unittest.TestCase):
    def test_disabled_accepts_both_expected_missing_imports(self):
        output = cargo_json(
            compiler_error("E0432", MODULE.EXPECTED_IMPORTS["AttemptCancellationToken"]),
            compiler_error("E0432", MODULE.EXPECTED_IMPORTS["WorkerRetry"]),
            {"reason": "build-finished", "success": False},
        )
        MODULE.evaluate_result(101, output, "", enabled=False, stage="worker disabled")

    def test_enabled_must_succeed_without_errors(self):
        MODULE.evaluate_result(0, '{"reason":"build-finished","success":true}\n', "", enabled=True, stage="enabled")
        with self.assertRaisesRegex(MODULE.GateCheckError, "worker-enabled"):
            MODULE.evaluate_result(101, cargo_json(compiler_error("E0308")), "", enabled=True, stage="worker-enabled")
        with self.assertRaisesRegex(MODULE.GateCheckError, "worker-enabled"):
            MODULE.evaluate_result(0, cargo_json(compiler_error("E0308")), "", enabled=True, stage="worker-enabled")

    def test_disabled_success_is_failure(self):
        with self.assertRaisesRegex(MODULE.GateCheckError, "unexpectedly succeeded"):
            MODULE.evaluate_result(0, '{"reason":"build-finished","success":true}', "", enabled=False, stage="disabled")

    def test_disabled_missing_only_one_symbol_is_failure(self):
        output = cargo_json(compiler_error("E0432", MODULE.EXPECTED_IMPORTS["WorkerRetry"]))
        with self.assertRaisesRegex(MODULE.GateCheckError, "AttemptCancellationToken"):
            MODULE.evaluate_result(101, output, "", enabled=False, stage="disabled")

    def test_matching_import_text_in_dependency_diagnostic_does_not_count(self):
        diagnostic = compiler_error("E0432", MODULE.EXPECTED_IMPORTS["AttemptCancellationToken"])
        diagnostic["message"]["spans"][0]["file_name"] = "dependency/src/main.rs"
        output = cargo_json(
            diagnostic,
            compiler_error("E0432", MODULE.EXPECTED_IMPORTS["WorkerRetry"]),
        )
        with self.assertRaisesRegex(MODULE.GateCheckError, "AttemptCancellationToken"):
            MODULE.evaluate_result(101, output, "", enabled=False, stage="disabled")

    def test_disabled_type_error_mixed_with_expected_imports_is_failure(self):
        output = cargo_json(
            compiler_error("E0432", MODULE.EXPECTED_IMPORTS["AttemptCancellationToken"]),
            compiler_error("E0432", MODULE.EXPECTED_IMPORTS["WorkerRetry"]),
            compiler_error("E0308", message="mismatched types"),
        )
        with self.assertRaisesRegex(MODULE.GateCheckError, "unexpected"):
            MODULE.evaluate_result(101, output, "", enabled=False, stage="disabled")

    def test_disabled_unrelated_failure_and_non_json_are_not_expected(self):
        with self.assertRaisesRegex(MODULE.GateCheckError, "unexpected"):
            MODULE.evaluate_result(101, cargo_json(compiler_error("E0432", message="unresolved import elsewhere")), "", enabled=False, stage="disabled")
        with self.assertRaisesRegex(MODULE.GateCheckError, "non-JSON"):
            MODULE.evaluate_result(101, "cargo failed before JSON", "", enabled=False, stage="disabled")

    def test_nonzero_without_compiler_diagnostic_is_failure(self):
        with self.assertRaisesRegex(MODULE.GateCheckError, "without compiler diagnostics"):
            MODULE.evaluate_result(101, '{"reason":"build-finished","success":false}', "offline dependency missing", enabled=False, stage="disabled")


class ProcessAndTemporaryConsumerTests(unittest.TestCase):
    def test_timeout_and_execution_errors_report_stage(self):
        with self.assertRaisesRegex(MODULE.GateCheckError, "timed out"):
            MODULE.run_cargo(["cargo"], cwd=Path("."), target=Path("target"), stage="worker enabled", runner=Mock(side_effect=subprocess.TimeoutExpired("cargo", 120)))
        with self.assertRaisesRegex(MODULE.GateCheckError, "could not execute cargo"):
            MODULE.run_cargo(["cargo"], cwd=Path("."), target=Path("target"), stage="worker enabled", runner=Mock(side_effect=OSError("missing")))

    def test_temp_consumer_uses_paths_with_spaces_and_is_cwd_independent(self):
        with tempfile.TemporaryDirectory(prefix="consumer path ") as temp_name:
            base = Path(temp_name)
            consumer = MODULE.prepare_consumer(base)
            manifest = (consumer / "Cargo.toml").read_text(encoding="utf-8")
            self.assertIn(str(MODULE.ROOT), manifest)
            calls = []

            def fake_run(command, **kwargs):
                calls.append((command, kwargs))
                if "fetch" in command:
                    return SimpleNamespace(returncode=0, stdout="", stderr="")
                if "--features" in command:
                    return SimpleNamespace(returncode=0, stdout=cargo_json({"reason": "build-finished", "success": True}), stderr="")
                diagnostics = [
                    compiler_error("E0432", MODULE.EXPECTED_IMPORTS[symbol])
                    for symbol in MODULE.EXPECTED_IMPORTS
                ]
                for diagnostic in diagnostics:
                    diagnostic["message"]["spans"][0]["file_name"] = "src/main.rs"
                return SimpleNamespace(returncode=101, stdout=cargo_json(*diagnostics), stderr="")

            MODULE.check(fetch=True, cargo="cargo executable", temp_root=base, runner=fake_run)
            commands = [call[0] for call in calls]
            self.assertEqual(len(commands), 3)
            self.assertEqual(commands[0][1], "fetch")
            for command, kwargs in calls:
                self.assertTrue(kwargs["cwd"].is_absolute())
                self.assertIn(" ", str(kwargs["cwd"]))
                if "--manifest-path" in command:
                    manifest_path = command[command.index("--manifest-path") + 1]
                    self.assertTrue(manifest_path.endswith("Cargo.toml"))
                    self.assertIn(" ", manifest_path)
                self.assertEqual(kwargs["timeout"], 120)
                self.assertEqual(kwargs["env"]["CARGO_TARGET_DIR"], str(kwargs["cwd"].parent / "target"))
                self.assertEqual(kwargs["env"]["LC_ALL"], "C")


if __name__ == "__main__":
    unittest.main()
