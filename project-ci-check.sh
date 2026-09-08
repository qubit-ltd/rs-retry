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

WORKER_DISABLED_LOG=$(mktemp)
trap 'rm -f "$WORKER_DISABLED_LOG"' EXIT
if cargo check --locked --manifest-path "$DOC_PROJECT_ROOT/tests/fixtures/worker_disabled/Cargo.toml" >"$WORKER_DISABLED_LOG" 2>&1; then
    cat "$WORKER_DISABLED_LOG"
    echo "worker API unexpectedly available without the worker feature" >&2
    exit 1
fi
cat "$WORKER_DISABLED_LOG"
if ! grep -q 'no `AttemptCancellationToken` in the root' "$WORKER_DISABLED_LOG" \
    || ! grep -q 'no method named `worker`' "$WORKER_DISABLED_LOG"; then
    echo "worker-disabled fixture failed for an unexpected reason" >&2
    exit 1
fi
