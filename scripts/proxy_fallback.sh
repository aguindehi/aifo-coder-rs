#!/usr/bin/env sh

# Clear proxy env vars when configured proxies are unreachable.

main() {
  if [ "${AIFO_PROXY_FORCE_PROXY:-0}" = "1" ] || [ "${AIFO_PROXY_FALLBACK:-1}" = "0" ]; then
    return 0
  fi

  proxy_keys="http_proxy https_proxy HTTP_PROXY HTTPS_PROXY"
  any_set=0
  for k in $proxy_keys; do
    v=$(eval "printf '%s' \"\${$k:-}\"")
    if [ -n "$v" ]; then
      any_set=1
      break
    fi
  done

  if [ "$any_set" -eq 0 ] || ! command -v python3 >/dev/null 2>&1; then
    return 0
  fi

  echo "Checking proxy settings..."

  if python3 - $proxy_keys <<'PY'
import os
import socket
import sys
import urllib.parse


def parse_target(raw):
    if not raw:
        return None
    s = raw.strip()
    if not s:
        return None
    if "://" not in s:
        s = "http://" + s
    try:
        parsed = urllib.parse.urlparse(s)
    except Exception:
        return None
    host = parsed.hostname
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    if not host:
        return None
    return host, port


def reachable(keys):
    for key in keys:
        target = parse_target(os.environ.get(key))
        if not target:
            continue
        host, port = target
        try:
            with socket.create_connection((host, port), 1):
                return True
        except OSError:
            continue
    return False


if __name__ == "__main__":
    sys.exit(0 if reachable(sys.argv[1:]) else 1)
PY
  then
    return 0
  fi

  echo "Proxy unreachable; clearing proxy env vars for direct access ..." >&2
  for k in $proxy_keys no_proxy NO_PROXY; do
    eval export "$k="
  done

  return 0
}

main "$@"
status=$?
return "$status" 2>/dev/null || exit "$status"
