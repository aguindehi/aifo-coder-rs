#!/bin/sh
# Phase 3 — Function renames (deterministic filtering)
# - unit_*.rs:   fn test_* -> fn unit_*
# - int_*.rs:    fn test_* -> fn int_*
# - e2e_*.rs:    fn test_* -> fn e2e_* and ensure #[ignore] before each #[test]
# Skips helpers under tests/support and tests/common.
set -eu

# Rename functions in Unit tests
for f in tests/unit_*.rs; do
  [ -f "$f" ] || continue
  perl -0777 -i -pe 's/(?m)^\s*fn\s+test_/fn unit_/g' "$f"
done

# Rename functions in Integration tests
for f in tests/int_*.rs; do
  [ -f "$f" ] || continue
  perl -0777 -i -pe 's/(?m)^\s*fn\s+test_/fn int_/g' "$f"
done

# Rename functions in E2E tests and ensure #[ignore] before each #[test]
for f in tests/e2e_*.rs; do
  [ -f "$f" ] || continue
  # Rename functions
  perl -0777 -i -pe 's/(?m)^\s*fn\s+test_/fn e2e_/g' "$f"
  # Ensure #[ignore] immediately before each #[test] (avoid duplicates)
  awk '
    BEGIN { prev="" }
    {
      if ($0 ~ /^[[:space:]]*#\[test\]/) {
        if (prev !~ /^[[:space:]]*#\[ignore\]/) {
          print "#[ignore]"
        }
        print $0
        prev = $0
      } else {
        print $0
        prev = $0
      }
    }
  ' "$f" > "$f.tmp" && mv "$f.tmp" "$f"
done

echo "Phase 3 rename complete: unit_/int_/e2e_ function prefixes enforced; e2e tests marked #[ignore] when missing."
