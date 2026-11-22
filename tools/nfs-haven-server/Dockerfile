# syntax=docker/dockerfile:1
# nfs-haven-server image (docs/nfs-haven-server.md). Build context: the repository root.
#   docker build -f tools/nfs-haven-server/Dockerfile -t nfs-haven-server .
# Contains no file of the original game: the game data is mounted read-only at run time.

ARG RUST_IMAGE=rust:1-slim-trixie
ARG DEBIAN_IMAGE=debian:trixie-slim

# --- 1. Build ---------------------------------------------------------------
FROM ${RUST_IMAGE} AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p nfs-haven-server \
 && cp target/release/nfs-haven-server /nfs-haven-server

# --- 2. Runtime -------------------------------------------------------------
FROM ${DEBIAN_IMAGE}
RUN apt-get update \
 && apt-get upgrade -y --no-install-recommends \
 && apt-get install -y --no-install-recommends tini \
 && rm -rf /var/lib/apt/lists/*

# Unprivileged user; the entrypoint changes its IDs to PUID/PGID at start (Unraid: 99/100).
RUN groupadd --gid 100 --non-unique nfsh \
 && useradd --uid 99 --gid 100 --non-unique --no-create-home --home-dir /data --shell /usr/sbin/nologin nfsh \
 && install -d -m 0755 /data /games

COPY --from=build /nfs-haven-server /usr/local/bin/nfs-haven-server
COPY tools/nfs-haven-server/docker/entrypoint.sh /usr/local/bin/entrypoint.sh

ENV NFSH_PORT=8642 \
    NFSH_GAMES_DIR=/games \
    NFSH_DATA_DIR=/data \
    PUID=99 PGID=100 UMASK=022 TZ=Etc/UTC

EXPOSE 8642
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD ["/usr/local/bin/nfs-haven-server", "--healthcheck"]

LABEL org.opencontainers.image.title="nfs-haven-server" \
      org.opencontainers.image.description="NFS Haven server: game data for every game, saves and the multiplayer lobby for the NFS Haven app (Apple TV, Mac)" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["nfs-haven-server"]
