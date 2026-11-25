#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
prompt="${1:-What do I owe this month? Show me all my bills.}"
mode="${POLYUI_DECODE_MODE:-grammar}"
port="${POLYUI_LLAMA_PORT:-8099}"
url="${POLYUI_LLAMA_URL:-}"
model="${POLYUI_LLAMA_MODEL:-}"
system="You are the assistant in the Karimov household's bills app in Tashkent. Amounts are in so'm. When a list, a choice or a chart helps, show UI with show_ui; otherwise answer in a short sentence."

usage() {
  cat <<EOF
This example sends one constrained request to a local llama-server and checks the answer with polyui validate.

Nothing to talk to: POLYUI_LLAMA_URL is not set and llama-server is not on PATH (or POLYUI_LLAMA_MODEL is not set).

1. Build llama.cpp with llguidance (needs Rust and cargo):
     git clone https://github.com/ggml-org/llama.cpp && cd llama.cpp
     cmake -B build -DLLAMA_LLGUIDANCE=ON && cmake --build build --config Release -j
2. Start it with a model, for example a Qwen3 GGUF:
     ./build/bin/llama-server -m /path/to/model.gguf --port 8099
3. Run this script against it:
     POLYUI_LLAMA_URL=http://127.0.0.1:8099 examples/local-model/run.sh "Show me my bills"
   or let the script start llama-server for you:
     POLYUI_LLAMA_MODEL=/path/to/model.gguf examples/local-model/run.sh "Show me my bills"

POLYUI_DECODE_MODE=json forces a show_ui answer through response_format instead of the text-or-call grammar.
EOF
}

server_pid=""
stop_server() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
}
trap stop_server EXIT

if [[ -z "$url" ]]; then
  if command -v llama-server >/dev/null 2>&1 && [[ -n "$model" ]]; then
    url="http://127.0.0.1:${port}"
    work_log="$(mktemp -t polyui-llama-server.XXXXXX)"
    echo "starting llama-server on port ${port} (log: ${work_log})"
    llama-server -m "$model" --port "$port" >"$work_log" 2>&1 &
    server_pid=$!
    for attempt in $(seq 1 120); do
      if curl -sf "${url}/health" >/dev/null 2>&1; then
        break
      fi
      if ! kill -0 "$server_pid" 2>/dev/null; then
        echo "llama-server exited, see ${work_log}" >&2
        exit 1
      fi
      if [[ "$attempt" == 120 ]]; then
        echo "llama-server was not ready after 120 s, see ${work_log}" >&2
        exit 1
      fi
      sleep 1
    done
  else
    usage
    exit 0
  fi
fi

cd "$root"
cargo build -q -p polyui -p polyui-decode
polyui="$root/target/debug/polyui"
decode="$root/target/debug/polyui-decode"
work="$(mktemp -d -t polyui-local-model.XXXXXX)"

"$polyui" tools --host bills > "$work/tools.json"

case "$mode" in
  grammar) target="llama-chat" ;;
  json) target="llama-json" ;;
  *) echo "POLYUI_DECODE_MODE must be grammar or json" >&2; exit 2 ;;
esac

"$decode" body "$target" --tools "$work/tools.json" --only show_ui \
  --system "$system" --user "$prompt" --max-tokens 2048 --temperature 0.2 > "$work/body.json"

echo "POST ${url}/v1/chat/completions (${mode} mode)"
curl -sS --fail-with-body "${url}/v1/chat/completions" \
  -H "Content-Type: application/json" \
  --data-binary @"$work/body.json" > "$work/response.json" || {
    echo "llama-server refused the request:" >&2
    cat "$work/response.json" >&2
    echo >&2
    echo "If it says llguidance is not enabled, rebuild llama.cpp with -DLLAMA_LLGUIDANCE=ON." >&2
    echo "A server built without it may stop instead of answering; check that it is still up." >&2
    exit 1
  }

"$decode" content --response "$work/response.json" > "$work/reply.txt"
echo "model reply:"
cat "$work/reply.txt"
echo

if [[ "$mode" == "json" ]]; then
  cp "$work/reply.txt" "$work/show_ui.json"
else
  set +e
  "$decode" parse --tools "$work/tools.json" --only show_ui --reply "$work/reply.txt" --arguments > "$work/show_ui.json"
  status=$?
  set -e
  if [[ "$status" == 3 ]]; then
    echo "The model answered in plain text, so there is no UI to validate."
    exit 0
  fi
  if [[ "$status" != 0 ]]; then
    echo "The reply could not be read as a tool call." >&2
    exit 1
  fi
fi

echo "polyui validate:"
"$polyui" validate "$work/show_ui.json" --host bills
