FROM rust:slim-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
ARG INFO_VERSION
ENV ETV_VERSION=${INFO_VERSION}
COPY . .
RUN cargo build --release

FROM ghcr.io/ersatztv/ersatztv-ffmpeg:7.1.1

RUN groupadd --gid 1000 ersatztv && \
    useradd --uid 1000 --gid 1000 --no-create-home --shell /sbin/nologin ersatztv && \
    mkdir -p /config && \
    chown ersatztv:ersatztv /config

ENV FONTCONFIG_PATH=/etc/fonts
RUN fc-cache -f

WORKDIR /app
COPY --from=build --chown=ersatztv:ersatztv \
    /build/target/release/ersatztv \
    /build/target/release/ersatztv-channel \
    /build/target/release/ersatztv-playout-generator \
    ./

USER ersatztv
ENTRYPOINT ["/app/ersatztv"]
CMD ["/config/lineup.json"]
