#!/usr/bin/env bash
# Re-record the GIFs at the top of the README.
#
#   docs/demo/record.sh            # all of them
#   docs/demo/record.sh add list   # just these
#
# Needs Docker and nothing else: gwt is built for Linux in a container, and the
# recording runs in another one (see Dockerfile for why not on the host).
set -euo pipefail

cd "$(dirname "$0")/.."/..
demo=docs/demo

rust_image=rust:1-slim-bookworm
vhs_image=gwt-vhs

tapes=("$@")
if [ ${#tapes[@]} -eq 0 ]; then
    tapes=(add list remove)
fi

echo "==> building gwt for linux"
docker run --rm \
    -v "$PWD:/src" \
    -w /src \
    -e CARGO_TARGET_DIR=/src/target/docker \
    "$rust_image" \
    cargo build --release --locked

echo "==> building the recording image"
docker build -q -t "$vhs_image" "$demo" >/dev/null

for tape in "${tapes[@]}"; do
    echo "==> recording $tape"
    docker run --rm \
        -v "$PWD/$demo:/demo" \
        -v "$PWD/target/docker/release/gwt:/usr/local/bin/gwt:ro" \
        -w /demo \
        "$vhs_image" \
        "$tape.tape"
done

echo "==> done"
ls -lh "$demo"/*.gif
