#!/bin/sh
set -eu

display="${OCG_BROWSER_DISPLAY:-:99}"
screen="${OCG_BROWSER_SCREEN:-1440x900x24}"
child_pids=""

cleanup() {
  trap - EXIT INT TERM
  for pid in $child_pids; do
    kill -TERM "$pid" 2>/dev/null || true
  done
  for pid in $child_pids; do
    wait "$pid" 2>/dev/null || true
  done
}

trap 'exit 0' INT TERM
trap cleanup EXIT

Xvfb "$display" -screen 0 "$screen" -nolisten tcp >/tmp/xvfb.log 2>&1 &
xvfb_pid=$!
child_pids="$child_pids $xvfb_pid"

display_number="${display#:}"
display_number="${display_number%%.*}"
ready=0
attempt=0
while [ "$attempt" -lt 100 ]; do
  if [ -S "/tmp/.X11-unix/X${display_number}" ]; then
    ready=1
    break
  fi
  if ! kill -0 "$xvfb_pid" 2>/dev/null; then
    echo "Xvfb exited before its display became ready" >&2
    exit 1
  fi
  attempt=$((attempt + 1))
  sleep 0.1
done
if [ "$ready" -ne 1 ]; then
  echo "timed out waiting for Xvfb" >&2
  exit 1
fi

DISPLAY="$display" openbox >/tmp/openbox.log 2>&1 &
openbox_pid=$!
child_pids="$child_pids $openbox_pid"
DISPLAY="$display" x11vnc \
  -display "$display" \
  -forever \
  -shared \
  -localhost \
  -rfbport 5900 \
  -nopw \
  -noxdamage \
  >/tmp/x11vnc.log 2>&1 &
x11vnc_pid=$!
child_pids="$child_pids $x11vnc_pid"

websockify --heartbeat=30 0.0.0.0:6080 127.0.0.1:5900 \
  >/tmp/websockify.log 2>&1 &
websockify_pid=$!
child_pids="$child_pids $websockify_pid"

/usr/local/bin/ocg-browser-worker &
worker_pid=$!
child_pids="$child_pids $worker_pid"

while :; do
  for process in \
    "Xvfb:$xvfb_pid" \
    "openbox:$openbox_pid" \
    "x11vnc:$x11vnc_pid" \
    "websockify:$websockify_pid" \
    "ocg-browser-worker:$worker_pid"
  do
    name="${process%%:*}"
    pid="${process#*:}"
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null || true
      echo "$name exited unexpectedly; stopping browser sidecar" >&2
      exit 1
    fi
  done
  sleep 1
done
