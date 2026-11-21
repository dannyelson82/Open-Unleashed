#!/bin/sh
# Generates OpenUnleashed.xcodeproj from project.yml. OU_BUNDLE_ID / OU_TEAM_ID come from the environment.
set -eu
cd "$(dirname "$0")"
: "${OU_BUNDLE_ID:=org.openunleashed.tvos}"
: "${OU_TEAM_ID:=}"
export OU_BUNDLE_ID OU_TEAM_ID
xcodegen generate --quiet
