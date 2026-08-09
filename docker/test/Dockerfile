FROM ghcr.io/ersatztv/ersatztv-ffmpeg:8.1.2 AS testbuild

RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates build-essential pkg-config jq \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable

# CARGO_MANIFEST_DIR is baked into the test binaries at compile time,
# so this path must be identical in the runtime stage.
WORKDIR /build
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    mkdir -p /out/tests && \
    cargo test --release --package ffpipeline --no-run --message-format=json \
      | jq -r 'select(.executable != null and .target.kind[0] == "test") | "\(.target.name) \(.executable)"' \
      | while read -r name exe; do \
            cp "$exe" "/out/tests/$name"; \
            strip "/out/tests/$name"; \
        done && \
    ls -la /out/tests

FROM ghcr.io/ersatztv/ersatztv-ffmpeg:8.1.2

COPY --from=testbuild /out/tests /app/tests
COPY crates/ffpipeline/tests/fixtures /build/crates/ffpipeline/tests/fixtures

COPY <<'SCRIPT' /app/run-tests
#!/bin/bash
# usage: run-tests [suite ...]   (default: all suites present)
suites=("$@")
[ ${#suites[@]} -eq 0 ] && suites=($(ls /app/tests))
rc=0
for s in "${suites[@]}"; do
    [ -x "/app/tests/$s" ] || { echo "no such suite: $s"; rc=1; continue; }
    echo "=============== $s ==============="
    "/app/tests/$s" --ignored --test-threads 1 --nocapture || rc=1
done
exit $rc
SCRIPT
RUN chmod +x /app/run-tests

ENTRYPOINT ["/app/run-tests"]
