FROM ghcr.io/typst/typst:0.14.2 AS typst
FROM rust:1.100-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src src
COPY templates templates
COPY assets assets
COPY migrations migrations
COPY reports reports
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked && cp target/release/taskapp /taskapp

FROM debian:bookworm-slim
RUN mkdir /data && chown 65532:65532 /data
COPY --from=build /taskapp /usr/local/bin/taskapp
COPY --from=typst /bin/typst /usr/local/bin/typst
USER 65532:65532
ENV TASKAPP_BIND=0.0.0.0:8080 TASKAPP_DB=/data/taskapp.sqlite
ENTRYPOINT ["taskapp"]
