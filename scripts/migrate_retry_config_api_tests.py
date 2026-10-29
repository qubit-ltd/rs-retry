"""Regression tests for retry configuration API migration."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "migrate_retry_config_api",
    Path(__file__).with_name("migrate_retry_config_api.py"),
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ImportMigrationTests(unittest.TestCase):
    def test_grouped_import_adds_each_required_symbol(self):
        for symbol in ("RetryConfig", "TokioRetry", "WorkerRetry"):
            for group in ("Retry, RetryPolicy", "\n    Retry,\n    RetryPolicy,\n"):
                with self.subTest(symbol=symbol, group=group):
                    source = (
                        "use qubit_retry::{" + group + "};\n"
                        + "let _ = " + symbol + "::new(&config);\n"
                    )
                    result = MODULE.add_imports(source)
                    imports = result.split("use qubit_retry::{", 1)[1].split("}", 1)[0]
                    entries = [entry.strip() for entry in imports.split(",")]
                    self.assertEqual(entries.count(symbol), 1)
                    self.assertEqual(MODULE.add_imports(result), result)

    def test_existing_grouped_symbol_is_unchanged(self):
        for symbol in ("RetryConfig", "TokioRetry", "WorkerRetry"):
            with self.subTest(symbol=symbol):
                source = (
                    "use qubit_retry::{Retry, " + symbol + "};\n"
                    + "let _ = " + symbol + "::new(&config);\n"
                )
                self.assertEqual(MODULE.add_imports(source), source)

    def test_similar_name_does_not_hide_missing_symbol(self):
        source = (
            "use qubit_retry::{RetryConfigBuilder};\n"
            "let _ = RetryConfig::builder();\n"
        )
        result = MODULE.add_imports(source)
        imports = result.split("use qubit_retry::{", 1)[1].split("}", 1)[0]
        self.assertIn("RetryConfig", [entry.strip() for entry in imports.split(",")])

    def test_separate_imports_remain_supported(self):
        source = "use qubit_retry::Retry;\nlet _ = RetryConfig::builder();\n"
        result = MODULE.add_imports(source)
        self.assertIn("use qubit_retry::RetryConfig;", result)
        self.assertEqual(MODULE.add_imports(result), result)

    def test_migrate_old_api_with_grouped_import(self):
        for old, facade in (("sync", "Retry"), ("tokio", "TokioRetry"), ("worker", "WorkerRetry")):
            with self.subTest(old=old):
                source = (
                    "use qubit_retry::{Retry, RetryPolicy};\n"
                    "let retry = Retry::<&str>::builder(policy).build();\n"
                    + "retry." + old + "().run(operation);\n"
                )
                self.assertTrue(MODULE.should_migrate(source))
                result = MODULE.migrate(source)
                self.assertIn("RetryConfig::<&str>::builder().policy(policy)", result)
                self.assertIn(facade + "::new(&retry).run(operation)", result)
                self.assertNotIn("." + old + "()", result)

    def test_multiline_facades_preserve_kind_and_are_idempotent(self):
        for old, facade in (("sync", "Retry"), ("tokio", "TokioRetry"), ("worker", "WorkerRetry")):
            with self.subTest(old=old):
                source = "retry\n    ." + old + "()\n    .run(operation);\n"
                result = MODULE.migrate(source)
                self.assertIn(facade + "::new(&retry)", result)
                self.assertNotIn("." + old + "()", result)
                self.assertEqual(MODULE.migrate(result), result)

    def test_multiline_facades_preserve_following_options_and_await(self):
        cases = (
            ("sync", "Retry", "    .hard_attempt_timeout(timeout)\n    .run(operation);"),
            ("tokio", "TokioRetry", "\n    .timer(timer)\n    .run(operation).await;"),
            ("worker", "WorkerRetry", "    .worker_thread_name(name)\n    .run(operation);"),
        )
        for old, facade, rest in cases:
            with self.subTest(old=old):
                source = "retry\n\n  ." + old + "()\n" + rest + "\n"
                result = MODULE.migrate(source)
                self.assertIn(facade + "::new(&retry)", result)
                self.assertIn(rest, result)
                self.assertEqual(MODULE.migrate(result), result)

    def test_same_line_facade_and_independent_value_remain_supported(self):
        source = "let executor = retry.tokio();\nlet result = retry.worker().run(operation);\n"
        result = MODULE.migrate(source)
        self.assertIn("let executor = TokioRetry::new(&retry);", result)
        self.assertIn("WorkerRetry::new(&retry).run(operation)", result)
        self.assertEqual(MODULE.migrate(result), result)


if __name__ == "__main__":
    unittest.main()
