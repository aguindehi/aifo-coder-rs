#!/bin/sh
# Rename tests to lane-prefixed filenames (unit_/int_/e2e_) and
# fix #[test] function names to begin with lane_ per v4 spec.
# Portable POSIX sh + awk/grep/sed only.

set -eu

is_lane_file() {
  case "$(basename "$1")" in
    unit_*.rs|int_*.rs|e2e_*.rs) return 0 ;;
    *) return 1 ;;
  esac
}

lane_from_filename() {
  base="$(basename "$1")"
  case "$base" in
    unit_*.rs) echo "unit" ;;
    int_*.rs) echo "int" ;;
    e2e_*.rs) echo "e2e" ;;
    *) echo "" ;;
  esac
}

lane_for_file_content() {
  f="$1"
  # E2E if any #[ignore]
  if grep -qE '#[[:space:]]*\[[[:space:]]*ignore[[:space:]]*\]' "$f"; then
    echo "e2e"; return
  fi
  # Integration markers
  if grep -qE 'std::process::Command|[^A-Za-z_]Command::new\(|container_runtime_path\(|toolexec_start_proxy\(' "$f"; then
    echo "int"; return
  fi
  echo "unit"
}

prefix_for_lane() {
  case "$1" in
    unit) echo "unit_" ;;
    int) echo "int_" ;;
    e2e) echo "e2e_" ;;
    *) echo "" ;;
  esac
}

fix_function_prefixes_in_file() {
  file="$1"
  lane="$2"
  pfx="$(prefix_for_lane "$lane")"

  # Skip helpers
  case "$file" in
    tests/support/*|tests/common/*) return 0 ;;
  esac

  tmp="$(mktemp)"
  awk -v pfx="$pfx" '
    BEGIN { in_test=0; }
    {
      # Pass through; decide changes only on the fn line after #[test]
      if ($0 ~ /#[[:space:]]*\[[[:space:]]*test[[:space:]]*\]/) {
        in_test=1; print; next;
      }
      if (in_test && $0 ~ /^[[:space:]]*fn[[:space:]]+[A-Za-z0-9_]/) {
        # Find end of "fn + spaces"
        lead_idx = match($0, /^[[:space:]]*fn[[:space:]]+/);
        lead_end = RSTART + RLENGTH - 1; # index in AWK is 1-based
        rest = substr($0, lead_end+1);
        # Name is the leading identifier in rest
        if (match(rest, /^[A-Za-z0-9_]+/)) {
          name = substr(rest, RSTART, RLENGTH);
          # Build new line if needed
          if (index(name, pfx) != 1) {
            newname = pfx name;
            prefix_text = substr($0, 1, lead_end);
            suffix_text = substr(rest, RLENGTH+1);
            print prefix_text newname suffix_text;
          } else {
            print;
          }
        } else {
          print;
        }
        in_test=0;
        next;
      }
      print;
    }
  ' "$file" > "$tmp"
  if ! cmp -s "$file" "$tmp" 2>/dev/null; then
    mv "$tmp" "$file"
  else
    rm -f "$tmp"
  fi
}

rename_file_to_lane() {
  f="$1"
  lane="$2"
  base="$(basename "$f")"
  dir="$(dirname "$f")"
  case "$base" in
    unit_*.rs|int_*.rs|e2e_*.rs)
      # Already lane-prefixed; ensure correctness only if mismatched
      case "$base" in
        unit_*.rs) cur="unit" ;;
        int_*.rs) cur="int" ;;
        e2e_*.rs) cur="e2e" ;;
      esac
      if [ "$cur" = "$lane" ]; then
        echo "keep: $f (lane=$lane)" >&2
        echo "$f"
        return 0
      fi
      # Mismatch: rename to desired lane
      new="$dir/$(prefix_for_lane "$lane")$(printf '%s' "$base" | sed -E 's/^(unit_|int_|e2e_)//')"
      ;;
    *)
      new="$dir/$(prefix_for_lane "$lane")$base"
      ;;
  esac

  if [ "$f" = "$new" ]; then
    echo "$f"
    return 0
  fi

  echo "mv: $f -> $new" >&2
  if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git mv -f "$f" "$new" 2>/dev/null || mv -f "$f" "$new"
  else
    mv -f "$f" "$new"
  fi
  echo "$new"
}

main() {
  # Find all test files except helpers
  files="$(find tests -type f -name '*.rs' ! -path 'tests/support/*' ! -path 'tests/common/*' | LC_ALL=C sort)"
  if [ -z "$files" ]; then
    echo "No test files found."
    exit 0
  fi

  for f in $files; do
    # Decide lane: prefer from filename if already lane-prefixed
    lane="$(lane_from_filename "$f")"
    if [ -z "$lane" ]; then
      lane="$(lane_for_file_content "$f")"
    fi

    # Rename if needed and fix function prefixes in the resulting file
    nf="$(rename_file_to_lane "$f" "$lane")"
    fix_function_prefixes_in_file "$nf" "$lane"
  done

  echo "Done. Consider reviewing the changes via git status/diff."
}

main "$@"
