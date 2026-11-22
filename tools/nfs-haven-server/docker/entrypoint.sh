#!/bin/sh
# nfs-haven-server entrypoint: starts as root only to apply PUID/PGID/UMASK and own /data, then drops privileges.
# Imports write into /games and take files from /inbox; nothing else of yours is changed.
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
  groupmod --non-unique --gid "$PGID" nfsh
  usermod --non-unique --uid "$PUID" --gid "$PGID" nfsh
  # only the server's own folder (saves, caches); fixed recursively only when PUID changed
  # the server's own folders (saves and caches, kept sources, staging): fixed recursively only when PUID changed.
  # /games and /inbox are yours (often a share): left as they are — the server user needs write access to them for
  # imports (it publishes sets into /games and takes files out of /inbox)
  for d in /data /sources /staging; do
    if [ -d "$d" ] && [ "$(stat -c %u "$d")" != "$PUID" ]; then
      chown -R "$PUID:$PGID" "$d"
    fi
  done
  umask "$UMASK"
  exec setpriv --reuid="$PUID" --regid="$PGID" --init-groups --inh-caps=-all --no-new-privs "$@"
fi

umask "$UMASK"
exec "$@"
