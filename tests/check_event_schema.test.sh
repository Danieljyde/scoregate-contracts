#!/usr/bin/env bash
# Direct tests for tools/check_event_schema.sh (Issue #200 & Issue #201).
#
# Validates that:
# 1. Topic schema changes across scoregate-score, scoregate-aggregator, mock-amm,
#    and mock-lending require an EVENT_VERSION bump.
# 2. Topic arity exceeding the 4-topic Soroban host limit fails validation.
# 3. Legitimate versioned changes and valid topic arities pass.
#
# Usage: tests/check_event_schema.test.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKER="$ROOT_DIR/tools/check_event_schema.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

REPO="$TMP_DIR/repo"
mkdir -p "$REPO/contracts/scoregate-score/src"
mkdir -p "$REPO/contracts/scoregate-aggregator/src"
mkdir -p "$REPO/contracts/mock-amm/src"
mkdir -p "$REPO/contracts/mock-lending/src"
mkdir -p "$REPO/tools"

cp "$CHECKER" "$REPO/tools/check_event_schema.sh"
chmod +x "$REPO/tools/check_event_schema.sh"

git -C "$REPO" init -q
git -C "$REPO" config user.email "checker-test@example.com"
git -C "$REPO" config user.name "Checker Test"

cat > "$REPO/contracts/scoregate-score/src/events.rs" <<'EOF'
pub const EVENT_VERSION: u32 = 1;
pub fn test_event(env: &Env) {
    env.events().publish((symbol_short!("ev"), EVENT_VERSION), 42);
}
EOF

cat > "$REPO/contracts/scoregate-aggregator/src/lib.rs" <<'EOF'
pub fn test_agg_event(env: &Env) {
    env.events().publish((symbol_short!("agg_ev"),), 42);
}
EOF

cat > "$REPO/contracts/mock-amm/src/events.rs" <<'EOF'
pub const EVENT_VERSION: u32 = 1;
pub fn test_amm_event(env: &Env) {
    env.events().publish((symbol_short!("amm_ev"), EVENT_VERSION), 42);
}
EOF

cat > "$REPO/contracts/mock-lending/src/events.rs" <<'EOF'
pub const EVENT_VERSION: u32 = 1;
pub fn test_lending_event(env: &Env) {
    env.events().publish((symbol_short!("lend_ev"), EVENT_VERSION), 42);
}
EOF

git -C "$REPO" add -A
git -C "$REPO" commit -q -m "initial base state"
BASE_SHA="$(git -C "$REPO" rev-parse HEAD)"

# ── Test 1: Clean base state passes ───────────────────────────────────────
if ! (cd "$REPO" && tools/check_event_schema.sh "$BASE_SHA" HEAD >/dev/null 2>&1); then
  echo "FAIL: base state should pass" >&2
  exit 1
fi
echo "PASS: base state passes"

# ── Test 2: Unversioned topic change in scoregate-score fails ─────────────
cat >> "$REPO/contracts/scoregate-score/src/events.rs" <<'EOF'
pub fn bad_event(env: &Env) {
    env.events().publish((symbol_short!("bad"),), 1);
}
EOF
git -C "$REPO" add -A && git -C "$REPO" commit -q -m "add unversioned event in score"
HEAD_SCORE="$(git -C "$REPO" rev-parse HEAD)"
if (cd "$REPO" && tools/check_event_schema.sh "$BASE_SHA" "$HEAD_SCORE" >/dev/null 2>&1); then
  echo "FAIL: unversioned topic change in scoregate-score must fail" >&2
  exit 1
fi
echo "PASS: unversioned topic change in scoregate-score rejected"

# ── Test 3: Unversioned topic change in mock-amm fails ────────────────────
git -C "$REPO" reset --hard "$BASE_SHA" -q
cat >> "$REPO/contracts/mock-amm/src/events.rs" <<'EOF'
pub fn bad_amm_event(env: &Env) {
    env.events().publish((symbol_short!("bad_amm"),), 1);
}
EOF
git -C "$REPO" add -A && git -C "$REPO" commit -q -m "add unversioned event in mock-amm"
HEAD_AMM="$(git -C "$REPO" rev-parse HEAD)"
if (cd "$REPO" && tools/check_event_schema.sh "$BASE_SHA" "$HEAD_AMM" >/dev/null 2>&1); then
  echo "FAIL: unversioned topic change in mock-amm must fail" >&2
  exit 1
fi
echo "PASS: unversioned topic change in mock-amm rejected"

# ── Test 4: Unversioned topic change in mock-lending fails ────────────────
git -C "$REPO" reset --hard "$BASE_SHA" -q
cat >> "$REPO/contracts/mock-lending/src/events.rs" <<'EOF'
pub fn bad_lending_event(env: &Env) {
    env.events().publish((symbol_short!("bad_lend"),), 1);
}
EOF
git -C "$REPO" add -A && git -C "$REPO" commit -q -m "add unversioned event in mock-lending"
HEAD_LEND="$(git -C "$REPO" rev-parse HEAD)"
if (cd "$REPO" && tools/check_event_schema.sh "$BASE_SHA" "$HEAD_LEND" >/dev/null 2>&1); then
  echo "FAIL: unversioned topic change in mock-lending must fail" >&2
  exit 1
fi
echo "PASS: unversioned topic change in mock-lending rejected"

# ── Test 5: Unversioned topic change in scoregate-aggregator fails ────────
git -C "$REPO" reset --hard "$BASE_SHA" -q
cat >> "$REPO/contracts/scoregate-aggregator/src/lib.rs" <<'EOF'
pub fn bad_agg_event(env: &Env) {
    env.events().publish((symbol_short!("bad_agg"),), 1);
}
EOF
git -C "$REPO" add -A && git -C "$REPO" commit -q -m "add unversioned event in aggregator"
HEAD_AGG="$(git -C "$REPO" rev-parse HEAD)"
if (cd "$REPO" && tools/check_event_schema.sh "$BASE_SHA" "$HEAD_AGG" >/dev/null 2>&1); then
  echo "FAIL: unversioned topic change in aggregator must fail" >&2
  exit 1
fi
echo "PASS: unversioned topic change in scoregate-aggregator rejected"

# ── Test 6: Versioned topic change in contracts passes ────────────────────
git -C "$REPO" reset --hard "$BASE_SHA" -q
cat >> "$REPO/contracts/mock-amm/src/events.rs" <<'EOF'
pub fn good_amm_event(env: &Env) {
    env.events().publish((symbol_short!("good"), EVENT_VERSION), 1);
}
EOF
git -C "$REPO" add -A && git -C "$REPO" commit -q -m "add versioned event in mock-amm"
HEAD_GOOD="$(git -C "$REPO" rev-parse HEAD)"
if ! (cd "$REPO" && tools/check_event_schema.sh "$BASE_SHA" "$HEAD_GOOD" >/dev/null 2>&1); then
  echo "FAIL: versioned topic change should pass" >&2
  exit 1
fi
echo "PASS: versioned topic change passes"

# ── Test 7: Topic arity exceeding 4 fails ─────────────────────────────────
git -C "$REPO" reset --hard "$BASE_SHA" -q
cat >> "$REPO/contracts/scoregate-score/src/events.rs" <<'EOF'
pub fn too_many_topics(env: &Env) {
    env.events().publish((symbol_short!("t1"), EVENT_VERSION, 1, 2, 3), 42);
}
EOF
git -C "$REPO" add -A && git -C "$REPO" commit -q -m "5 topics event"
HEAD_ARITY="$(git -C "$REPO" rev-parse HEAD)"
if (cd "$REPO" && tools/check_event_schema.sh "$BASE_SHA" "$HEAD_ARITY" >/dev/null 2>&1); then
  echo "FAIL: 5-topic event must fail arity check" >&2
  exit 1
fi
echo "PASS: 5-topic event rejected by arity check"

echo "ALL TESTS PASSED for tools/check_event_schema.sh"
