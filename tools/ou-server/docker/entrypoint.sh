#!/bin/sh
# ou-server entrypoint: starts as root only to apply PUID/PGID/UMASK and own /data, then drops privileges.
# The game, mods and HD folders are mounted read-only and never changed.
set -eu

PUID="${PUID:-99}"
PGID="${PGID:-100}"
UMASK="${UMASK:-022}"

case "$PUID$PGID" in
  *[!0-9]*) echo "PUID and PGID must be numbers" >&2; exit 1 ;;
esac
case "$UMASK" in
  *[!0-7]*) echo "UMASK must be an octal number such as 022" >&2; exit 1 ;;
esac

if [ "$(id -u)" = "0" ]; then
  if [ "$PUID" = "0" ]; then
    echo "Refusing to run as root (PUID=0). Use Unraid's default PUID=99 PGID=100." >&2
    exit 1
  fi
  groupmod --non-unique --gid "$PGID" ou
  usermod --non-unique --uid "$PUID" --gid "$PGID" ou
  # only the server's own folder (saves, caches); fixed recursively only when PUID changed
  if [ "$(stat -c %u /data)" != "$PUID" ]; then
    chown -R "$PUID:$PGID" /data
  fi
  umask "$UMASK"
  exec setpriv --reuid="$PUID" --regid="$PGID" --init-groups --inh-caps=-all --no-new-privs "$@"
fi

umask "$UMASK"
exec "$@"
