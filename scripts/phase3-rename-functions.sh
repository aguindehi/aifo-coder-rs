#!/bin/sh
# Phase 3 — Function renames (deterministic filtering)
# - unit_*.rs:   rename #[test] fns test_* -> unit_*
# - int_*.rs:    rename #[test] fns test_* -> int_*
# - e2e_*.rs:    rename #[test] fns test_* -> e2e_* and ensure #[ignore] before each #[test]
# Skips helpers under tests/support and tests/common.
set -eu

# Rename only functions annotated with #[test] to avoid renaming helpers.

# Unit lane
for f in tests/unit_*.rs; do
  [ -f "$f" ] || continue
  perl -0777 -i -pe '
    s{
      (^\s*(?:\#\s*\[[^\]]+\]\s*\n\s*)*)   # attributes before fn
      fn \s+ test_
    }{
      my $attrs = $1;
      ($attrs =~ /#\s*\[\s*test\s*\]/i) ? ($attrs . "fn unit_") : $&
    }egmx
  ' "$f"
done

# Integration lane
for f in tests/int_*.rs; do
  [ -f "$f" ] || continue
  perl -0777 -i -pe '
    s{
      (^\s*(?:\#\s*\[[^\]]+\]\s*\n\s*)*)   # attributes before fn
      fn \s+ test_
    }{
      my $attrs = $1;
      ($attrs =~ /#\s*\[\s*test\s*\]/i) ? ($attrs . "fn int_") : $&
    }egmx
  ' "$f"
done

# E2E lane: add #[ignore] if missing, and rename only #[test] fns
for f in tests/e2e_*.rs; do
  [ -f "$f" ] || continue
  # Ensure #[ignore] exists above each #[test] if not already present among preceding attrs
  perl -0777 -i -pe '
    s{
      (^\s*(?:\#\s*\[[^\]]+\]\s*\n\s*)*)   # attributes block
      \#\s*\[\s*test\s*\]
    }{
      my $attrs = $1;
      ($attrs =~ /#\s*\[\s*ignore\s*\]/i) ? ($attrs . "#[test]") : ($attrs . "#[ignore]\n#[test]")
    }egmx
  ' "$f"
  # Collapse duplicate #[ignore] entries before a #[test], even with blank lines between
  perl -0777 -i -pe '
    s/#\[\s*ignore\s*\]\s*(?:\n\s*)+#\[\s*ignore\s*\](?=\s*\n\s*#\s*\[\s*test\s*\])/#[ignore]/g
  ' "$f"
  # Rename only #[test] functions
  perl -0777 -i -pe '
    s{
      (^\s*(?:\#\s*\[[^\]]+\]\s*\n\s*)*)   # attributes before fn
      fn \s+ test_
    }{
      my $attrs = $1;
      ($attrs =~ /#\s*\[\s*test\s*\]/i) ? ($attrs . "fn e2e_") : $&
    }egmx
  ' "$f"
done

echo "Phase 3 rename complete: lane prefixes enforced on #[test] fns; e2e tests marked #[ignore] without duplicates."
