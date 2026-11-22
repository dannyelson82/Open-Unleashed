#!/bin/sh
# Builds the Mac harness: ReNFS's files unchanged + the host (no SDL). GPL-3.0-or-later.
set -eu
H="$(cd "$(dirname "$0")/.." && pwd)"
R="$H/../../engines/renfs/port/src"
OUT="${1:-$H/../../target/renfs-host}"
mkdir -p "$OUT"
SRCS=$(sed -n '/add_executable(renfs/,/^)/p' "$H/../../engines/renfs/port/CMakeLists.txt" | grep -oE 'src/[a-z0-9_]+\.c' | sed "s|^src/|$R/|")
# ReNFS's main is renamed renfs_main (only in its own files); the harness keeps its main
OBJ="$OUT/obj"; mkdir -p "$OBJ"
for f in $SRCS; do clang -c -O2 -fwrapv -std=c11 -w -Dmain=renfs_main -I"$H/include" -I"$R" -o "$OBJ/$(basename "$f" .c).o" "$f"; done
clang -O2 -std=c11 -Wall -Wextra -I"$H/include" -I"$H" -o "$OUT/renfs-harness" "$OBJ"/*.o "$H/host.c" "$H/test/harness.c" -lpthread -lobjc
echo "$OUT/renfs-harness"
