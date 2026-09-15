# syntax=docker/dockerfile:1

# The ARMv7 FFmpeg base image is Jammy. Build on Jammy for all platforms so the
# binaries link against its glibc. Build on BUILDPLATFORM so the compiler does
# not run under QEMU.
FROM --platform=$BUILDPLATFORM ubuntu:22.04 AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl build-essential cmake pkg-config jq \
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable
RUN cargo install --locked cargo-chef

ARG TARGETARCH
ARG TARGETVARIANT
ARG BUILDARCH
RUN case "$TARGETARCH/$TARGETVARIANT" in \
        amd64/) target=x86_64-unknown-linux-gnu; compiler=x86_64-linux-gnu; pkg=x86-64-linux-gnu ;; \
        arm64/|arm64/v8) target=aarch64-unknown-linux-gnu; compiler=aarch64-linux-gnu; pkg=$compiler ;; \
        arm/v7) target=armv7-unknown-linux-gnueabihf; compiler=arm-linux-gnueabihf; pkg=$compiler ;; \
        *) echo "Unsupported target: $TARGETARCH/$TARGETVARIANT" >&2; exit 1 ;; \
    esac && \
    if [ "$BUILDARCH" != "$TARGETARCH" ]; then \
        apt-get update && apt-get install -y --no-install-recommends "gcc-$pkg" "g++-$pkg" && \
        rm -rf /var/lib/apt/lists/*; \
    fi && \
    rustup target add "$target" && \
    printf 'export CARGO_BUILD_TARGET=%s\nexport TARGET_CC=%s-gcc\nexport TARGET_CXX=%s-g++\nexport TARGET_AR=%s-ar\nexport TARGET_STRIP=%s-strip\n' \
        "$target" "$compiler" "$compiler" "$compiler" "$compiler" > /target-env
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS dependencies
COPY --from=planner /build/recipe.json recipe.json
# Do not use a RUN cache mount here. The GitHub cache exporter does not save
# the contents of cache mounts.
RUN . /target-env && cargo chef cook --locked --release --recipe-path recipe.json

FROM dependencies AS build
ARG INFO_VERSION
ENV ETV_VERSION=${INFO_VERSION}
COPY . .
RUN . /target-env && cargo build --locked --release && \
    mkdir -p /out && \
    cp target/$CARGO_BUILD_TARGET/release/ersatztv \
       target/$CARGO_BUILD_TARGET/release/ersatztv-channel \
       target/$CARGO_BUILD_TARGET/release/ersatztv-playout-generator /out/

FROM dependencies AS test-dependencies
RUN . /target-env && cargo chef cook --locked --release --package ffpipeline --tests --recipe-path recipe.json

FROM test-dependencies AS testbuild
COPY . .
# Write the JSON to a file first. A pipe would hide a failed compilation.
RUN . /target-env && \
    cargo test --locked --release --package ffpipeline --no-run --message-format=json > /tmp/tests.json && \
    mkdir -p /out/tests && \
    jq -r 'select(.executable != null and .target.kind[0] == "test") | "\(.target.name) \(.executable)"' /tmp/tests.json \
      | while read -r name exe; do \
            cp "$exe" "/out/tests/$name"; \
            "$TARGET_STRIP" "/out/tests/$name" || exit 1; \
        done && \
    test -x /out/tests/software

FROM ghcr.io/ersatztv/ersatztv-ffmpeg:8.1.2 AS test-runtime

COPY --from=testbuild /out/tests /app/tests
# The test binaries contain the compile-time CARGO_MANIFEST_DIR under /build.
COPY crates/ffpipeline/tests/fixtures /build/crates/ffpipeline/tests/fixtures

COPY <<'SCRIPT' /app/run-tests
#!/bin/bash
# usage: run-tests [suite ...] [filter ...] [-- libtest-args ...]
#   run-tests                     all suites, all tests
#   run-tests cuda qsv            those suites, all tests
#   run-tests cuda tonemap        cuda suite, tests matching "tonemap"
#   run-tests tonemap             all suites, tests matching "tonemap"
#   run-tests cuda -- --nocapture cuda suite, extra libtest args
# Leading arguments that match a suite name select suites. All other arguments
# go to each test binary.
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

FROM ghcr.io/ersatztv/ersatztv-ffmpeg:8.1.2 AS runtime

# The ARMv7 base image already has a group with GID 1000.
RUN (getent group 1000 || groupadd --gid 1000 ersatztv) && \
    useradd --uid 1000 --gid 1000 --no-create-home --shell /sbin/nologin ersatztv && \
    mkdir -p /config && \
    chown 1000:1000 /config

ENV FONTCONFIG_PATH=/etc/fonts
RUN fc-cache -f

WORKDIR /app
COPY --from=build --chown=1000:1000 \
    /out/ersatztv \
    /out/ersatztv-channel \
    /out/ersatztv-playout-generator \
    ./

USER ersatztv
ENTRYPOINT ["/app/ersatztv"]
CMD ["/config/lineup.json"]
