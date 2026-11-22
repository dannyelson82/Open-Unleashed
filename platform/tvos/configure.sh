#!/bin/sh
# Generates OpenUnleashed.xcodeproj from project.yml. NFSH_BUNDLE_ID / NFSH_TEAM_ID come from the environment.
set -eu
cd "$(dirname "$0")"
: "${NFSH_BUNDLE_ID:=com.dannyelson.nfshaven}"
: "${NFSH_TEAM_ID:=}"
export NFSH_BUNDLE_ID NFSH_TEAM_ID
xcodegen generate --quiet
