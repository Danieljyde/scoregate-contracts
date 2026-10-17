#!/usr/bin/env bash
set -euo pipefail

BASE_SHA="${1:-}"
HEAD_SHA="${2:-HEAD}"

if [ -z "$BASE_SHA" ]; then
  echo "usage: $0 <base-sha> [head-sha]" >&2
  exit 2
fi

if ! git cat-file -e "$BASE_SHA^{commit}" 2>/dev/null; then
  echo "event schema check: base commit '$BASE_SHA' is unavailable" >&2
  exit 2
fi

# Check event definitions across all contracts in the repository:
# scoregate-score, mock-amm, mock-lending (contracts/*/src/events.rs),
# and scoregate-aggregator (contracts/scoregate-aggregator/src/lib.rs).
EVENT_FILES=()
for f in contracts/*/src/events.rs; do
  [ -f "$f" ] && EVENT_FILES+=("$f")
done
if [ -f "contracts/scoregate-aggregator/src/lib.rs" ]; then
  EVENT_FILES+=("contracts/scoregate-aggregator/src/lib.rs")
fi

# Ensure all 4 required contract files are tracked
for required_file in \
  "contracts/scoregate-score/src/events.rs" \
  "contracts/scoregate-aggregator/src/lib.rs" \
  "contracts/mock-amm/src/events.rs" \
  "contracts/mock-lending/src/events.rs"; do
  found=false
  for ef in "${EVENT_FILES[@]}"; do
    if [ "$ef" = "$required_file" ]; then
      found=true
      break
    fi
  done
  if [ "$found" = false ] && [ -f "$required_file" ]; then
    EVENT_FILES+=("$required_file")
  fi
done

# ── 1. Schema versioning check on diff ─────────────────────────────────────
# For every event file, verify topic edits are accompanied by an EVENT_VERSION marker.
for events_file in "${EVENT_FILES[@]}"; do
  schema_diff="$(git diff --unified=0 "$BASE_SHA" "$HEAD_SHA" -- "$events_file")"
  if [ -z "$schema_diff" ]; then
    continue
  fi

  # Event topic edits are schema edits. Require the explicit version marker in
  # the same change so indexers cannot receive a new topic shape as version 1.
  if printf '%s\n' "$schema_diff" \
    | grep -E '^[+-][^+-].*(publish\(|symbol_short!|Symbol::new)' \
    | grep -v '^[+-][^+-].*EVENT_VERSION' >/dev/null 2>&1; then
    if ! printf '%s\n' "$schema_diff" | grep -E '^[+-][^+-].*EVENT_VERSION' >/dev/null 2>&1; then
      echo "event schema check failed: event topic code changed without an EVENT_VERSION bump in $events_file" >&2
      exit 1
    fi
  fi
done

# ── 2. Topic arity check (Soroban host limit <= 4 topics) ──────────────────
# Soroban host limits the topic tuple of any emitted event to at most 4 ScVal elements.
python3 - "${EVENT_FILES[@]}" <<'EOF'
import sys

files = sys.argv[1:]
max_topics = 4
failed = False

for file_path in files:
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            lines = f.readlines()
    except Exception as e:
        print(f"event schema check: error reading {file_path}: {e}", file=sys.stderr)
        sys.exit(2)

    for i, line in enumerate(lines):
        if "publish(" in line:
            buf = ""
            for j in range(i, min(i + 20, len(lines))):
                buf += lines[j]
                if ";" in lines[j]:
                    break
            idx = buf.find("publish(")
            if idx == -1:
                continue
            sub = buf[idx + len("publish("):]
            depth = 0
            arg1 = ""
            for char in sub:
                if char in "([":
                    depth += 1
                elif char in ")]":
                    depth -= 1
                    if depth < 0:
                        break
                elif char == "," and depth == 0:
                    break
                arg1 += char
            arg1 = arg1.strip()
            if arg1.startswith("(") and arg1.endswith(")"):
                inner = arg1[1:-1].strip()
                d = 0
                count = 0
                has_content = False
                for c in inner:
                    if c in "([":
                        d += 1
                    elif c in ")]":
                        d -= 1
                    elif c == "," and d == 0:
                        count += 1
                    elif not c.isspace():
                        has_content = True
                if has_content:
                    count += 1
                if count > max_topics:
                    print(
                        f"topic arity check failed: {file_path}:{i+1} has {count} topics, "
                        f"exceeding Soroban limit of {max_topics}: {arg1}",
                        file=sys.stderr,
                    )
                    failed = True

if failed:
    sys.exit(1)
EOF

echo "event schema stability check passed"
