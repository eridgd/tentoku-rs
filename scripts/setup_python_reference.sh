#!/usr/bin/env sh
# Setup Python reference (tentoku) for cross-validation and vs_python benchmarks.
# Clones https://github.com/eridgd/tentoku, checks out cython-version, and
# installs so that tests/benches can import tentoku via sys.path.insert(0, reference).
# Idempotent: skips clone/install if reference/ exists and verification passes.
set -e

REPO_ROOT="${1:-.}"
REFERENCE_DIR="${REPO_ROOT}/reference"
TENTOKU_REPO="https://github.com/eridgd/tentoku.git"
BRANCH="cython-version"

# Resolve absolute path for Python sys.path (must be parent of tentoku package dir)
case "$REFERENCE_DIR" in
    /*) ;;
    *) REFERENCE_DIR="$(cd "$REPO_ROOT" && pwd)/reference" ;;
esac
TENTOKU_PKG_DIR="${REFERENCE_DIR}/tentoku"
PYTHON_DB_PATH="${TENTOKU_PYTHON_DB:-${TENTOKU_PKG_DIR}/data/jmdict.python.db}"

cd "$REPO_ROOT"

# Verify: can we import the modules that cross_validate and vs_python benchmarks use?
verify_reference() {
    py="${1:-python3}"
    REFERENCE_DIR="$REFERENCE_DIR" "$py" -c '
import sys, os
sys.path.insert(0, os.environ["REFERENCE_DIR"])
from tentoku.sqlite_dict_optimized import OptimizedSQLiteDictionary
from tentoku.tokenizer import tokenize
' 2>/dev/null
}

ensure_python_db() {
    py="${1:-python3}"
    mkdir -p "$(dirname "$PYTHON_DB_PATH")"

    if [ -s "$PYTHON_DB_PATH" ]; then
        echo "Python comparison DB already present: $PYTHON_DB_PATH"
        return 0
    fi

    echo "Building Python comparison DB at: $PYTHON_DB_PATH"
    REFERENCE_DIR="$REFERENCE_DIR" TENTOKU_PYTHON_DB="$PYTHON_DB_PATH" "$py" -c '
import os, sys
sys.path.insert(0, os.environ["REFERENCE_DIR"])
from tentoku.build_database import build_database
ok = build_database(
    os.environ["TENTOKU_PYTHON_DB"],
    xml_path=None,
    show_progress=True,
    auto_download=True
)
raise SystemExit(0 if ok else 1)
'
}

PY_BIN="${PYTHON_BIN:-python3}"
if [ -d "$TENTOKU_PKG_DIR" ] && [ -f "$TENTOKU_PKG_DIR/__init__.py" ]; then
    if verify_reference "$PY_BIN"; then
        if ensure_python_db "$PY_BIN"; then
            echo "reference/ already present and verified (tentoku importable + dedicated DB ready)"
            exit 0
        fi
        echo "warning: reference/ importable but dedicated Python DB build failed"
        exit 0
    fi
    # Exists but verification failed; try install and re-verify below
    if [ -d "$TENTOKU_PKG_DIR/.git" ]; then
        (cd "$TENTOKU_PKG_DIR" && git fetch -q origin "$BRANCH" 2>/dev/null || true)
        (cd "$TENTOKU_PKG_DIR" && git checkout -q "$BRANCH" 2>/dev/null || true)
    fi
else
    # Clone into reference/tentoku so sys.path.insert(0, reference) finds package "tentoku"
    if [ -d "$TENTOKU_PKG_DIR" ]; then
        rm -rf "$TENTOKU_PKG_DIR"
    fi
    mkdir -p "$REFERENCE_DIR"
    git clone --depth 1 --branch "$BRANCH" "$TENTOKU_REPO" "$TENTOKU_PKG_DIR"
    echo "Cloned tentoku ($BRANCH) into reference/tentoku/"
fi

# Install in place so Cython extensions build and any deps are available.
# Best-effort: if pip fails (e.g. PEP 668), reference/ may still work via sys.path.
if command -v pip3 >/dev/null 2>&1; then
    (cd "$TENTOKU_PKG_DIR" && pip3 install -e . -q 2>/dev/null) && echo "Installed reference tentoku (pip3 install -e .)" || true
elif command -v pip >/dev/null 2>&1; then
    (cd "$TENTOKU_PKG_DIR" && pip install -e . -q 2>/dev/null) && echo "Installed reference tentoku (pip install -e .)" || true
fi

# Final verification: ensure reference is usable
if verify_reference "$PY_BIN"; then
    if ensure_python_db "$PY_BIN"; then
        echo "reference/ ready and verified (dedicated Python DB ready)"
    else
        echo "warning: reference/ ready but dedicated Python DB build failed"
    fi
else
    echo "warning: reference/ present but import check failed (tests/benches may still work if deps are installed)"
fi
