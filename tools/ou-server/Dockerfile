# syntax=docker/dockerfile:1
# ou-server image (docs/ou-server.md). Build context: the repository root.
#   docker build -f tools/ou-server/Dockerfile -t ou-server .
# Contains no file of the original game: the game data is mounted read-only at run time.

ARG RUST_IMAGE=rust:1-slim-trixie
ARG DEBIAN_IMAGE=debian:trixie-slim

# --- 1. Build ---------------------------------------------------------------
FROM ${RUST_IMAGE} AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p ou-server \
 && cp target/release/ou-server /ou-server

# --- 2. Runtime -------------------------------------------------------------
FROM ${DEBIAN_IMAGE}
RUN apt-get update \
 && apt-get upgrade -y --no-install-recommends \
 && apt-get install -y --no-install-recommends tini \
 && rm -rf /var/lib/apt/lists/*

# Unprivileged user; the entrypoint changes its IDs to PUID/PGID at start (Unraid: 99/100).
RUN groupadd --gid 100 --non-unique ou \
 && useradd --uid 99 --gid 100 --non-unique --no-create-home --home-dir /data --shell /usr/sbin/nologin ou \
 && install -d -m 0755 /data /game /mods /hd

COPY --from=build /ou-server /usr/local/bin/ou-server
COPY tools/ou-server/docker/entrypoint.sh /usr/local/bin/entrypoint.sh

ENV OU_PORT=8642 \
    OU_GAME_DIR=/game \
    OU_MODS_DIR=/mods \
    OU_HD_DIR=/hd \
    OU_DATA_DIR=/data \
    PUID=99 PGID=100 UMASK=022 TZ=Etc/UTC

EXPOSE 8642
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD ["/usr/local/bin/ou-server", "--healthcheck"]

LABEL org.opencontainers.image.title="ou-server" \
      org.opencontainers.image.description="OpenUnleashed server: game data, saves and multiplayer lobby for the OpenUnleashed Apple TV and Mac builds" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["ou-server"]
