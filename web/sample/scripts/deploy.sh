#!/usr/bin/env bash
# Build the collector, push it, and roll the chart forward.
set -euo pipefail

REGISTRY="${REGISTRY:-ghcr.io/example}"
IMAGE="collector"
NAMESPACE="${NAMESPACE:-signals}"

log() {
  printf '[deploy] %s\n' "$*" >&2
}

require() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    log "$tool is not on PATH"
    exit 1
  fi
}

version() {
  git describe --tags --always --dirty
}

build() {
  local tag="$1"
  log "building $REGISTRY/$IMAGE:$tag"
  docker build -t "$REGISTRY/$IMAGE:$tag" .
}

push() {
  local tag="$1"
  log "pushing $REGISTRY/$IMAGE:$tag"
  docker push "$REGISTRY/$IMAGE:$tag"
}

roll() {
  local tag="$1"
  log "rolling $NAMESPACE to $tag"
  helm upgrade --install "$IMAGE" ./chart \
    --namespace "$NAMESPACE" \
    --set "image.tag=$tag"
}

main() {
  require docker
  require helm
  require git
  local tag
  tag="$(version)"
  build "$tag"
  push "$tag"
  roll "$tag"
  log "done"
}

main "$@"
