#!/bin/bash
# Project-owned bilingual documentation checks, invoked by rs-ci.
set -euo pipefail

DOC_PROJECT_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
if ! command -v python3 > /dev/null 2>&1; then
    echo "Python 3.11 or newer is required for documentation example checks" >&2
    exit 1
fi
python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else "Python 3.11 or newer is required")'
python3 -B "$DOC_PROJECT_ROOT/scripts/check_doc_examples_tests.py"
python3 -B "$DOC_PROJECT_ROOT/scripts/check_doc_examples.py"
