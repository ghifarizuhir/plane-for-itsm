#!/bin/bash
set -e
DJANGO=${1:-http://localhost:8000/health}
RUST=${2:-http://localhost:8001/health}
echo "Comparing Django: $DJANGO vs Rust: $RUST"
DJANGO_BODY=$(curl -s "$DJANGO" || echo '{"status":"django-down"}')
RUST_BODY=$(curl -s "$RUST" || echo '{"status":"rust-down"}')
echo "Django: $DJANGO_BODY"
echo "Rust: $RUST_BODY"
if command -v jq >/dev/null 2>&1; then
  diff <(echo "$DJANGO_BODY" | jq -S . 2>/dev/null || echo "$DJANGO_BODY") <(echo "$RUST_BODY" | jq -S . 2>/dev/null || echo "$RUST_BODY") && echo "parity ok" || echo "parity diff (expected until full parity)"
else
  [ "$DJANGO_BODY" = "$RUST_BODY" ] && echo "parity ok" || echo "parity diff"
fi
