#!/usr/bin/env bash
# Drive happy-cracking CLIs in an isolated tmux PTY and keep proof outside scratch.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
TMUX_CONF="/exec-daemon/tmux.portal.conf"
if [[ ! -f "$TMUX_CONF" ]]; then
  TMUX_CONF=""
fi

tmux_cmd() {
  if [[ -n "$TMUX_CONF" ]]; then
    tmux -f "$TMUX_CONF" "$@"
  else
    tmux "$@"
  fi
}

usage() {
  cat <<'EOF'
Usage:
  hc-verify.sh build
  hc-verify.sh doctor [--bin NAME]
  hc-verify.sh drive --run-id ID --feature NAME [--bin NAME] -- ARGS...
  hc-verify.sh cleanup --run-id ID

Proof is written to /tmp/hc-verify-proof/<run-id>/<feature>/ and is not removed by cleanup.
Scratch for a run is /tmp/hc-verify-scratch/<run-id>/. The tmux session is hc-verify-<run-id>.
EOF
}

bin_path() {
  local name="${1:-happy-cracking}"
  printf '%s\n' "$ROOT/target/debug/$name"
}

cmd_build() {
  cargo build --manifest-path "$ROOT/Cargo.toml" --bin happy-cracking --bin hgrep --bin hfind
}

cmd_doctor() {
  local name="happy-cracking"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --bin)
        name="$2"
        shift 2
        ;;
      *)
        echo "unknown doctor arg: $1" >&2
        exit 2
        ;;
    esac
  done
  local bin
  bin="$(bin_path "$name")"
  if [[ ! -x "$bin" ]]; then
    echo "doctor: missing executable $bin" >&2
    exit 1
  fi
  local help
  help="$("$bin" --help)"
  case "$name" in
    happy-cracking)
      grep -q "CTF toolkit" <<<"$help"
      ;;
    hgrep | hg | hrg)
      grep -q "grep-compatible" <<<"$help"
      ;;
    hfind | hfd)
      grep -q -- "--gitignore" <<<"$help"
      ;;
    *)
      echo "doctor: unsupported bin $name" >&2
      exit 2
      ;;
  esac
  printf 'bin=%s\n' "$bin"
  printf 'mtime=%s\n' "$(stat -c %y "$bin")"
  printf 'doctor=ok\n'
}

cmd_drive() {
  local run_id="" feature="" name="happy-cracking"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --run-id)
        run_id="$2"
        shift 2
        ;;
      --feature)
        feature="$2"
        shift 2
        ;;
      --bin)
        name="$2"
        shift 2
        ;;
      --)
        shift
        break
        ;;
      *)
        echo "unknown drive arg: $1" >&2
        exit 2
        ;;
    esac
  done
  if [[ -z "$run_id" || -z "$feature" || $# -eq 0 ]]; then
    usage >&2
    exit 2
  fi
  local bin scratch proof session
  bin="$(bin_path "$name")"
  scratch="/tmp/hc-verify-scratch/$run_id"
  proof="/tmp/hc-verify-proof/$run_id/$feature"
  session="hc-verify-$run_id"
  if [[ ! -x "$bin" ]]; then
    echo "drive: missing executable $bin (run build and doctor first)" >&2
    exit 1
  fi
  mkdir -p "$scratch" "$proof"
  printf '%q ' "$bin" "$@" >"$proof/command.txt"
  printf '\n' >>"$proof/command.txt"
  cat >"$scratch/invoke.sh" <<EOF
#!/usr/bin/env bash
set +e
$(printf '%q ' "$bin" "$@") 2>$(printf '%q' "$proof/stderr.txt") | tee $(printf '%q' "$proof/stdout.txt")
status=\${PIPESTATUS[0]}
if [[ -s $(printf '%q' "$proof/stderr.txt") ]]; then
  cat $(printf '%q' "$proof/stderr.txt") >&2
fi
printf '%s\n' "\$status" >$(printf '%q' "$proof/exit_code.txt")
exit "\$status"
EOF
  chmod +x "$scratch/invoke.sh"
  if tmux_cmd has-session -t "=$session" 2>/dev/null; then
    echo "drive: session $session already exists; pick a new run id" >&2
    exit 1
  fi
  tmux_cmd new-session -d -s "$session" -c "$scratch" -- \
    bash -lc "script -q -e -f -c $(printf '%q' "$scratch/invoke.sh") $(printf '%q' "$proof/transcript.txt"); exec sleep 30"
  tmux_cmd set-option -t "$session" remain-on-exit on >/dev/null
  local i
  for i in $(seq 1 50); do
    if [[ -f "$proof/exit_code.txt" ]]; then
      break
    fi
    sleep 0.1
  done
  if [[ ! -f "$proof/exit_code.txt" ]]; then
    echo "drive: timed out waiting for $proof/exit_code.txt" >&2
    exit 1
  fi
  # Let script flush the transcript after the child exits.
  sleep 0.2
  tmux_cmd capture-pane -t "$session" -p -S - >"$proof/pane.txt" || true
  printf 'proof=%s\n' "$proof"
  printf 'exit=%s\n' "$(cat "$proof/exit_code.txt")"
  printf 'stdout=%s\n' "$(cat "$proof/stdout.txt")"
}

cmd_cleanup() {
  local run_id=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --run-id)
        run_id="$2"
        shift 2
        ;;
      *)
        echo "unknown cleanup arg: $1" >&2
        exit 2
        ;;
    esac
  done
  if [[ -z "$run_id" ]]; then
    usage >&2
    exit 2
  fi
  local session="hc-verify-$run_id"
  if tmux_cmd has-session -t "=$session" 2>/dev/null; then
    tmux_cmd kill-session -t "=$session"
  fi
  rm -rf "/tmp/hc-verify-scratch/$run_id"
  printf 'cleaned=%s\n' "$run_id"
  printf 'proof_kept=/tmp/hc-verify-proof/%s\n' "$run_id"
}

main() {
  local cmd="${1:-}"
  if [[ -z "$cmd" ]]; then
    usage >&2
    exit 2
  fi
  shift
  case "$cmd" in
    build) cmd_build "$@" ;;
    doctor) cmd_doctor "$@" ;;
    drive) cmd_drive "$@" ;;
    cleanup) cmd_cleanup "$@" ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
