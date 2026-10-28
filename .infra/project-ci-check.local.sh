#!/usr/bin/env bash
set -euo pipefail
project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
python3 -B "$project_root/scripts/check_doc_examples_tests.py"
python3 -B "$project_root/scripts/check_doc_examples.py" --fetch
