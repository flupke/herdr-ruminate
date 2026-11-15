#!/usr/bin/env bash
# Opens the Explore page at <url> in this computer's browser through `<control> open-url`, the
# review pane's own choice of browser. With a machine, a port and an ssh command after it, it
# first forwards that port of this computer's loopback interface to the same port of the
# machine, unless a forward it started for them still answers. The forward lives while ssh's
# remote command, which waits for the page to stop answering there, and herdr-app, which runs
# this script, both live; ssh asks nothing (BatchMode) and fails when it cannot listen here
# (ExitOnForwardFailure). The page key of herdr-app-mod/explore-page/main.js runs it, and shows
# what it prints on its standard error when it fails. Bash, for its /dev/tcp.
#
# Usage: open-page.sh <control> <url> [<machine> <port> <ssh command...>]
set -u
control=$1
url=$2
shift 2

# Whether a program of this computer listens on port $1 of its loopback interface.
answers() {
  (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null
}

# Starts the forward `$@` for the record $1, in the background with no stream of the mod's exec,
# which would wait for them to close, and in a session of its own where setsid exists (not on
# macOS): a watcher that writes its process ID in the record, runs ssh, and ends it once
# herdr-app ($2) ends.
start_forward() {
  local detach=()
  command -v setsid >/dev/null && detach=(setsid)
  "${detach[@]}" bash -c '
    record=$1 app=$2
    shift 2
    echo $$ >"$record.new" && mv -f "$record.new" "$record"
    "$@" </dev/null >/dev/null 2>"$record.log" &
    ssh=$!
    while kill -0 "$ssh" 2>/dev/null && kill -0 "$app" 2>/dev/null; do sleep 1; done
    kill "$ssh" 2>/dev/null
    [ "$(cat "$record" 2>/dev/null)" = "$$" ] && rm -f "$record"' bash "$@" </dev/null >/dev/null 2>&1 &
}

# Forwards port $2 from machine $1 with the ssh command that follows, or reuses the forward that
# still does it; fails with the reason on its standard error.
forward() {
  local machine=$1 port=$2 forwards record watcher tries
  shift 2
  forwards="${XDG_RUNTIME_DIR:-$HOME/.cache}/herdr-progressive-reviewer/forwards"
  if ! mkdir -p "$forwards" || ! chmod 700 "$forwards"; then
    echo "cannot write $forwards" >&2
    return 1
  fi
  record="$forwards/${machine//[^A-Za-z0-9._-]/_}-$port"
  watcher=$(cat "$record" 2>/dev/null)
  # The watcher of the record, by its command line, since its process ID may now be another's.
  if [ -n "$watcher" ] && ps -o args= -p "$watcher" 2>/dev/null | grep -qF -- "$record" && answers "$port"; then
    return 0
  fi
  if answers "$port"; then
    echo "port $port is in use on this computer" >&2
    return 1
  fi
  rm -f "$record" "$record.log"
  start_forward "$record" "$PPID" "$@"
  tries=0
  while [ ! -s "$record" ] && [ "$tries" -lt 50 ]; do
    sleep 0.1
    tries=$((tries + 1))
  done
  watcher=$(cat "$record" 2>/dev/null)
  if [ -z "$watcher" ]; then
    echo "the ssh forward of port $port did not start" >&2
    return 1
  fi
  tries=0
  until answers "$port"; do
    if ! kill -0 "$watcher" 2>/dev/null || [ "$tries" -ge 100 ]; then
      echo "the ssh forward of port $port did not start: $(tail -n 3 "$record.log" 2>/dev/null | tr '\n' ' ')" >&2
      return 1
    fi
    sleep 0.1
    tries=$((tries + 1))
  done
}

# Opens the page; a browser still running after 3 s, as one that stays in the foreground, has
# opened it.
open_url() {
  local log browser tries status
  log=$(mktemp)
  "$control" open-url "$url" </dev/null >/dev/null 2>"$log" &
  browser=$!
  tries=0
  while kill -0 "$browser" 2>/dev/null; do
    if [ "$tries" -ge 30 ]; then
      rm -f "$log"
      return 0
    fi
    sleep 0.1
    tries=$((tries + 1))
  done
  wait "$browser"
  status=$?
  [ "$status" -ne 0 ] && sed 's/^Error: //' "$log" >&2
  rm -f "$log"
  return "$status"
}

if [ $# -gt 0 ]; then
  forward "$@" || exit 1
fi
open_url
