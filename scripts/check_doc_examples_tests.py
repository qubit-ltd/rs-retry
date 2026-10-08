"""Regression tests for the documentation executable-block contract."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("check_doc_examples", Path(__file__).with_name("check_doc_examples.py"))
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ExampleContractTests(unittest.TestCase):
    def parse(self, text):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "guide.zh_CN.md"
            path.write_text(text, encoding="utf-8")
            return MODULE.extract(path)

    def test_unannotated_rust_is_rejected_with_location(self):
        with self.assertRaisesRegex(ValueError, r"guide.zh_CN.md:1.*annotation"):
            self.parse("```rust\nfn main() {}\n```\n")

    def test_both_feature_requirement_and_source_line_survive(self):
        examples = self.parse("# 指南\n\n<!-- retry-example: kind=run features=tokio,serde -->\n```rust\nfn main() {}\n```\n")
        self.assertEqual(examples[0]["features"], ["tokio", "serde"])
        self.assertEqual(examples[0]["line"], 5)
        self.assertEqual(examples[0]["code"], "fn main() {}\n")

    def test_worker_feature_is_accepted(self):
        examples = self.parse("<!-- retry-example: kind=run features=worker -->\n```rust\nfn main() {}\n```\n")
        self.assertEqual(examples[0]["features"], ["worker"])

    def test_async_feature_is_accepted(self):
        examples = self.parse("<!-- retry-example: kind=cargo features=async -->\n```toml\n[dependencies]\nqubit-retry = { version = \"0.24\", features = [\"async\"] }\n```\n")
        self.assertEqual(examples[0]["features"], ["async"])

    def test_legacy_annotation_has_no_consumer_dependencies(self):
        examples = self.parse("<!-- retry-example: kind=run features=none -->\n```rust\nfn main() {}\n```\n")
        self.assertEqual(examples[0]["deps"], [])

    def test_clock_dependency_is_parsed(self):
        examples = self.parse("<!-- retry-example: kind=run features=none deps=clock -->\n```rust\nfn main() {}\n```\n")
        self.assertEqual(examples[0]["deps"], ["clock"])

    def test_clock_test_util_dependency_is_parsed(self):
        examples = self.parse("<!-- retry-example: kind=run features=none deps=clock-test-util -->\n```rust\nfn main() {}\n```\n")
        self.assertEqual(examples[0]["deps"], ["clock-test-util"])

    def test_futures_executor_dependency_is_declared_for_async_example(self):
        manifest = self.consumer_manifest("kind=run features=async deps=futures-executor")
        self.assertIn('futures-executor = "0.3"', manifest)
        self.assertNotIn('tokio = ', manifest)

    def test_invalid_dependency_declarations_fail_with_location(self):
        cases = (
            ("kind=run features=none deps=unknown", "rust", "unknown or repeated"),
            ("kind=run features=none deps=clock,clock", "rust", "unknown or repeated"),
            ("kind=run features=none deps=", "rust", "empty consumer dependency"),
            ("kind=run features=none deps=clock deps=clock-test-util", "rust", "repeated deps"),
            ("kind=cargo features=none deps=clock", "toml", "Cargo example cannot"),
        )
        for annotation, language, message in cases:
            with self.subTest(annotation=annotation):
                with self.assertRaisesRegex(ValueError, rf"guide.zh_CN.md:2:.*{message}"):
                    self.parse(f"<!-- retry-example: {annotation} -->\n```{language}\nexample\n```\n")

    def test_consumer_manifest_omits_undeclared_clock_dependency(self):
        self.assertNotIn("qubit-clock", self.consumer_manifest("kind=run features=none"))

    def consumer_manifest(self, annotation):
        example = self.parse(f"<!-- retry-example: {annotation} -->\n```rust\nfn main() {{}}\n```\n")[0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nversion = "0.26.0"\n', encoding="utf-8")
            return MODULE.consumer_manifest(root, example)

    def test_consumer_manifest_adds_declared_clock_dependency(self):
        manifest = self.consumer_manifest("kind=run features=none deps=clock")
        self.assertIn('qubit-clock = "0.13"', manifest)
        self.assertIn('serde_json = "1"', manifest)

    def test_consumer_manifest_adds_test_util_clock_dependency(self):
        manifest = self.consumer_manifest("kind=run features=none deps=clock-test-util")
        self.assertIn('qubit-clock = { version = "0.13", features = ["test-util"] }', manifest)

    def test_consumer_manifest_preserves_tokio_fixture(self):
        manifest = self.consumer_manifest("kind=run features=tokio")
        self.assertIn('tokio = { version = "1.52", features = ["rt", "macros", "time"] }', manifest)
        self.assertIn('serde_json = "1"', manifest)

    def test_bilingual_retry_session_guides_declare_test_clock_dependency(self):
        root = Path(__file__).parents[1]
        for name, heading in (
            ("user_guide.md", "### Let your scheduler own the wait"),
            ("user_guide.zh_CN.md", "### 让现有调度器负责等待"),
        ):
            with self.subTest(document=name):
                content = (root / "doc" / name).read_text(encoding="utf-8")
                section = content.split(heading, 1)[1].split("\n### ", 1)[0]
                example_offset = section.index("use qubit_clock::ManualMonotonicClock;")
                example_annotation = section.rfind("<!-- retry-example:", 0, example_offset)
                self.assertEqual(
                    section[example_annotation:section.index("\n", example_annotation)],
                    "<!-- retry-example: kind=run features=none deps=clock-test-util -->",
                )
                self.assertIn("cargo add qubit-clock@0.13 --features test-util", section[:example_annotation])

    def test_rust_modifiers_cannot_silently_skip_validation(self):
        with self.assertRaisesRegex(ValueError, "unsupported.*fence"):
            self.parse("```rust,ignore\nfn main() {}\n```\n")

    def test_unknown_features_and_unclosed_blocks_fail(self):
        with self.assertRaisesRegex(ValueError, "unknown feature"):
            self.parse("<!-- retry-example: kind=run features=magic -->\n```rust\nfn main() {}\n```\n")
        with self.assertRaisesRegex(ValueError, "unclosed"):
            self.parse("<!-- retry-example: kind=run features=none -->\n```rust\nfn main() {}\n")

    def test_cargo_annotation_cannot_hide_feature_mismatch(self):
        example = self.parse('<!-- retry-example: kind=cargo features=none -->\n```toml\n[dependencies]\nqubit-retry = { version = "0.22", features = ["tokio"] }\n```\n')[0]
        with self.assertRaisesRegex(ValueError, "features disagree"):
            MODULE.cargo_dependency(example, "0.22.0")

    def test_cargo_example_version_must_match_current_release(self):
        example = self.parse('<!-- retry-example: kind=cargo features=none -->\n```toml\n[dependencies]\nqubit-retry = "0.21"\n```\n')[0]
        with self.assertRaisesRegex(ValueError, "version"):
            MODULE.cargo_dependency(example, "0.22.0")


if __name__ == "__main__":
    unittest.main()
