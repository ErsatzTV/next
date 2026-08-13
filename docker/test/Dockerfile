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
# usage: run-tests [suite ...] [filter ...] [-- libtest-args ...]
#   run-tests                     all suites, all tests
#   run-tests cuda qsv            those suites, all tests
#   run-tests cuda tonemap        cuda suite, tests matching "tonemap"
#   run-tests tonemap             all suites, tests matching "tonemap"
#   run-tests cuda -- --nocapture cuda suite, extra libtest args
# Leading args that name an existing suite select suites; everything after the
# first non-suite arg is forwarded to each test binary.
suites=()
passthru=()
while [ $# -gt 0 ]; do
    if [ "$1" = "--" ]; then
        shift
        passthru+=("$@")
        break
    fi
    if [ ${#passthru[@]} -eq 0 ] && [ -x "/app/tests/$1" ]; then
        suites+=("$1")
    else
        passthru+=("$1")
    fi
    shift
done
[ ${#suites[@]} -eq 0 ] && suites=($(ls /app/tests))
rc=0
for s in "${suites[@]}"; do
    echo "=============== $s ${passthru[*]} ==============="
    "/app/tests/$s" --ignored --test-threads 1 "${passthru[@]}" || rc=1
done
exit $rc
SCRIPT
RUN chmod +x /app/run-tests

ENTRYPOINT ["/app/run-tests"]
