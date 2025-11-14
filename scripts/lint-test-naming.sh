#!/bin/sh
# Optional enforcement (lint) for test naming conventions (Phase 6 of v4 spec)
# Validates:
# - File names start with unit_/int_/e2e_ (except tests/support and tests/common)
# - #[test] function names in lane files start with unit_/int_/e2e_
# - Unit files must not spawn external processes (Command::new, std::process::Command)
# Exits non-zero on issues when run with --strict or LINT_STRICT=1

set -eu

strict=0
if [ "${1:-}" = "--strict" ] || [ "${LINT_STRICT:-0}" = "1" ]; then
  strict=1
fi

issues=0

# Collect test files excluding helpers
list_test_files() {
  find tests -type f -name '*.rs' \
    ! -path 'tests/support/*' \
    ! -path 'tests/common/*' \
    | LC_ALL=C sort
}

# Heuristic lane prediction for non-lane files: e2e if any #[ignore], else int; unless clearly unit
predict_lane() {
  f="$1"
  if grep -qE 'std::process::Command|[^A-Za-z_]Command::new\(' "$f"; then
    if grep -q '\#\[ignore\]' "$f"; then
      echo "e2e"
    else
      echo "int"
    fi
    return
  fi
  if grep -qE 'toolexec_start_proxy|container_runtime_path\(' "$f"; then
    if grep -q '\#\[ignore\]' "$f"; then
      echo "e2e"
    else
      echo "int"
    fi
    return
  fi
  # No obvious integration markers: assume unit
  echo "unit"
}

# Check #[test] function names in lane files
check_function_prefixes() {
  f="$1"
  expected="$2" # unit_ | int_ | e2e_
  awk -v pfx="$expected" -v file="$f" '
    BEGIN { in_test=0; }
    {
      # detect #[test]
      if ($0 ~ /#[[:space:]]*\[[[:space:]]*test[[:space:]]*\]/) { in_test=1; next; }

      # next non-attribute line that declares fn <name>(...)
      if (in_test && $0 ~ /^[[:space:]]*fn[[:space:]]+[A-Za-z0-9_]/) {
        line=$0
        # strip leading spaces + "fn " prefix
        sub(/^[[:space:]]*fn[[:space:]]+/, "", line)
        # extract identifier up to first non-identifier char
        if (match(line, /^[A-Za-z0-9_]+/)) {
          name = substr(line, RSTART, RLENGTH)
          if (index(name, pfx) != 1) {
            print "NAMING: " file ":" NR ": #[test] function " name " should begin with " pfx
          }
        }
        in_test=0
      }
    }
  ' "$f"
}

# Unit files must not spawn external processes
check_unit_no_spawns() {
  f="$1"
  grep -nE 'std::process::Command|[^A-Za-z_]Command::new\(' "$f" || true
}

echo "== Test naming lint (optional enforcement) =="
echo "Strict mode: $strict"
echo

non_lane_files=0
func_prefix_violations=0
unit_spawn_violations=0

for f in $(list_test_files); do
  base="$(basename "$f")"
  case "$base" in
    unit_*.rs) lane="unit" ;;
    int_*.rs)  lane="int" ;;
    e2e_*.rs)  lane="e2e" ;;
    *) lane="" ;;
  esac

  if [ -z "$lane" ]; then
    non_lane_files=$((non_lane_files+1))
    predicted="$(predict_lane "$f")"
    hint="$(dirname "$f")/$(printf '%s_%s' "$predicted" "$base")"
    echo "NAMING: $f: file name should start with unit_/int_/e2e_ (predicted lane: $predicted)"
    echo "        hint: git mv '$f' '$hint'"
    continue
  fi

  # Enforce function prefixes inside lane files
  expected="${lane}_"
  out="$(check_function_prefixes "$f" "$expected")"
  if [ -n "$out" ]; then
    func_prefix_violations=$((func_prefix_violations+1))
    echo "$out"
  fi

  # Enforce unit files have no process spawns
  if [ "$lane" = "unit" ]; then
    us="$(check_unit_no_spawns "$f")"
    if [ -n "$us" ]; then
      unit_spawn_violations=$((unit_spawn_violations+1))
      echo "UNIT-SPAWN: $f uses external process spawning (not allowed in unit tests):"
      echo "$us" | sed 's/^/  /'
      echo "           hint: reclassify to int_*.rs or remove process spawning"
    fi
  fi
done

echo
echo "Summary:"
echo "  files without lane prefix: $non_lane_files"
echo "  #[test] function prefix violations: $func_prefix_violations"
echo "  unit files with process spawns: $unit_spawn_violations"

total=$((non_lane_files + func_prefix_violations + unit_spawn_violations))
if [ "$total" -gt 0 ]; then
  echo
  echo "Found $total test naming issues."
  if [ "$strict" -eq 1 ]; then
    exit 1
  fi
fi

echo "OK (naming lint completed)"
exit 0
