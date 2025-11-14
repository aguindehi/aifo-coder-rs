#!/bin/sh
# Phase 3 — Function renames (deterministic filtering)
# - unit_*.rs:   fn test_* -> fn unit_*
# - int_*.rs:    fn test_* -> fn int_*
# - e2e_*.rs:    fn test_* -> fn e2e_* and ensure #[ignore] before each #[test]
# Skips helpers under tests/support and tests/common.
set -eu

is_empty_glob() {
  # Returns 0 if no files matched (to avoid literal patterns being treated as filenames)
  [ "$#" -eq 1 ] && [ "$1" = "$2" ]
}

# Rename functions in Unit tests
UNIT_GLOB=$(printf "%s" tests/unit_*.rs)
if ! is_empty_glob "$UNIT_GLOB" "tests/unit_*.rs"; then
  for f in $UNIT_GLOB; do
    # Replace function names starting with test_ at line starts (common for #[test] fns)
    perl -0777 -pe 's/(?m)^\s*fn\s+test_/fn unit_/g' -i "$f"
  done
fi

# Rename functions in Integration tests
INT_GLOB=$(printf "%s" tests/int_*.rs)
if ! is_empty_glob "$INT_GLOB" "tests/int_*.rs"; then
  for f in $INT_GLOB; do
    perl -0777 -pe 's/(?m)^\s*fn\s+test_/fn int_/g' -i "$f"
  done
fi

# Rename functions in E2E tests and ensure #[ignore]
E2E_GLOB=$(printf "%s" tests/e2e_*.rs)
if ! is_empty_glob "$E2E_GLOB" "tests/e2e_*.rs"; then
  for f in $E2E_GLOB; do
    perl -0777 -pe 's/(?m)^\s*fn\s+test_/fn e2e_/g' -i "$f"
    # Ensure an #[ignore] exists immediately before #[test] lines; then deduplicate
    # Insert #[ignore] before every #[test]
    sed -i 's/#\[test\]/#[ignore]\n#[test]/g' "$f"
    # Collapse duplicate ignores possibly introduced above
    # Reduce sequences #[ignore]\n#[ignore] to a single #[ignore]
    sed -i ':a;N;$!ba;s/#\[ignore\]\n#\[ignore\]/#[ignore]/g' "$f"
    # Reduce #[ignore]\n#[ignore]\n#[test] to #[ignore]\n#[test]
    sed -i ':a;N;$!ba;s/#\[ignore\]\n#\[ignore\]\n#\[test\]/#[ignore]\n#[test]/g' "$f"
  done
fi

echo "Phase 3 rename complete: unit_/int_/e2e_ function prefixes enforced; e2e tests marked #[ignore]."
