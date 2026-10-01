#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ ! -f sha256sums.txt ]]; then
  echo "sha256sums.txt not found" >&2
  exit 1
fi
sha256sum -c sha256sums.txt
for tar in tars/*.tar; do
  echo "Loading $tar..."
  docker load -i "$tar"
done
echo "All images loaded."
