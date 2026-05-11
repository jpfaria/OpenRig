#!/usr/bin/env bash
set -euo pipefail

echo "Running cargo llvm-cov (html report)..."
cargo llvm-cov --workspace --html --output-dir coverage/

REPORT="coverage/html/index.html"
if [ ! -f "$REPORT" ]; then
  echo "Report not found at $REPORT"
  exit 1
fi

echo "Opening coverage report..."
if command -v open &>/dev/null; then
  open "$REPORT"
elif command -v xdg-open &>/dev/null; then
  xdg-open "$REPORT"
else
  echo "Report generated at: $REPORT"
fi
