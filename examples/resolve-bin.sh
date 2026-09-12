# Sourced by example scripts. Expects ROOT to be set.
# Picks the newest local debug/release binary so `cargo build -q` wins over a
# stale target/release leftover. Override with AUDIOSTEGO=/path/to/audiostego.

resolve_bin() {
  if [[ -n "${AUDIOSTEGO:-}" ]]; then
    if [[ -x "$AUDIOSTEGO" ]]; then
      echo "$AUDIOSTEGO"
      return
    fi
    echo "AUDIOSTEGO is set but not executable: $AUDIOSTEGO" >&2
    exit 1
  fi

  local td="${CARGO_TARGET_DIR:-$ROOT/target}"
  local candidates=()
  [[ -x "$td/release/audiostego" ]] && candidates+=("$td/release/audiostego")
  [[ -x "$td/debug/audiostego" ]] && candidates+=("$td/debug/audiostego")
  [[ -x "$ROOT/target/release/audiostego" ]] && candidates+=("$ROOT/target/release/audiostego")
  [[ -x "$ROOT/target/debug/audiostego" ]] && candidates+=("$ROOT/target/debug/audiostego")

  local newest=""
  local c
  for c in "${candidates[@]}"; do
    if [[ -z "$newest" || "$c" -nt "$newest" ]]; then
      newest="$c"
    fi
  done

  if [[ -n "$newest" ]]; then
    echo "$newest"
    return
  fi
  if command -v audiostego >/dev/null 2>&1; then
    command -v audiostego
    return
  fi
  echo "audiostego binary not found; run: cargo build" >&2
  exit 1
}
