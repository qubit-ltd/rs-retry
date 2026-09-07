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
