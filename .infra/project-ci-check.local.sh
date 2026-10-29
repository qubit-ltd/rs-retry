#!/usr/bin/env bash
set -euo pipefail
project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
python3 -B "$project_root/scripts/check_doc_examples_tests.py"
python3 -B "$project_root/scripts/check_doc_examples.py" --fetch
python3 -B "$project_root/scripts/migrate_retry_config_api_tests.py"
python3 -B "$project_root/scripts/check_worker_feature_gate_tests.py"
python3 -B "$project_root/scripts/check_worker_feature_gate.py" --fetch
