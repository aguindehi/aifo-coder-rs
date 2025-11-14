#!/bin/sh
set -eu
# Normalize attribute blocks in e2e_* tests to have exactly one #[ignore] before #[test].
for f in tests/e2e_*.rs; do
  [ -f "$f" ] || continue
  perl -0777 -i -pe '
    s{
      (^\s*(?:\#\s*\[[^\]]+\]\s*\n\s*)*)   # attributes block
      \#\s*\[\s*test\s*\]
    }{
      my $attrs = $1;
      # Remove any #[ignore] lines in the block
      $attrs =~ s/^\s*\#\s*\[\s*ignore\s*\].*\n//mg;
      $attrs . "#[ignore]\n#[test]"
    }egmx
  ' "$f"
done
echo "De-duplicated #[ignore] attributes in e2e_* tests."
