#!/bin/sh
set -eu
codesign --force --sign 'guckpg Local Development Code Signing' --identifier de.parndt.guckpg "$1"
exec "$@"
