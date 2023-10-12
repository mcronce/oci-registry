#!/bin/bash

# Expects oci-registry to be listening on localhost:8080, for example:
# RUST_LOG=info,actix-web=debug cargo run -- --listen 0.0.0.0:8080 filesystem --root /tmp/oci-mirror

set -eu

test() {
  echo "--- Testing $1/$2"

  rm -rf /tmp/oci-mirror

  # containerd
  url="localhost:8080/v2/$2?ns=$1"
  curl -s -o /dev/null -w "  %{http_code}: $url\n" "$url"

  rm -rf /tmp/oci-mirror

  # cri-o
  url="localhost:8080/v2/$1/$2"
  curl -s -o /dev/null -w "  %{http_code}: $url\n" "$url"

  rm -rf /tmp/oci-mirror

  # Special Docker Hub case falling back to default_ns
  if [ "$1" == "docker.io" ]; then
    url="localhost:8080/v2/$2"
    curl -s -o /dev/null -w "  %{http_code}: $url\n" "$url"
  fi
}

test 'docker.io' 'envoyproxy/envoy/manifests/v1.26.2'
test 'docker.io' 'library/busybox/manifests/latest'
test 'docker.io' 'grafana/mimirtool/blobs/sha256:31e352740f534f9ad170f75378a84fe453d6156e40700b882d737a8f4a6988a3'

test 'gcr.io' 'distroless/static/manifests/latest'
test 'gcr.io' 'distroless/static/blobs/sha256:fe5ca62666f04366c8e7f605aa82997d71320183e99962fa76b3209fdfbb8b58'
test 'gcr.io' 'flame-public/buildbuddy-app-onprem/manifests/sha256:68932b6227b4c16d6bbc08997620284ef71f7b10ee9b9dc47b631ae92f31a6a3'

test 'ghcr.io' 'buildbarn/bb-runner-installer/manifests/sha256:beb72a39662a613a889f17e7a15254f7e03849e4f7000f399ea7bdabe4a25f79'
test 'ghcr.io' 'buildbarn/bb-runner-installer/blobs/sha256:e2334dd9fee4b77e48a8f2d793904118a3acf26f1f2e72a3d79c6cae993e07f0'
