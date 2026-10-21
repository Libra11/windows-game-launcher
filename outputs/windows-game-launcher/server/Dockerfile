# syntax=docker/dockerfile:1
ARG RUST_IMAGE=rust:1-bookworm
FROM ${RUST_IMAGE} AS build
WORKDIR /build
COPY server/Cargo.toml server/Cargo.lock ./server/
COPY server/src ./server/src
COPY server/migrations ./server/migrations
COPY cloud-protocol/Cargo.toml ./cloud-protocol/
COPY cloud-protocol/src ./cloud-protocol/src
COPY save-archive/Cargo.toml ./save-archive/
COPY save-archive/src ./save-archive/src
RUN cargo build --locked --release --manifest-path server/Cargo.toml --bin youji-cloud-server

# 从同发行版构建镜像提取 curl 及运行库，运行镜像无需联网安装软件。
FROM build AS runtime-assets
RUN mkdir -p /runtime-assets/etc/ssl/certs \
    && cp /etc/ssl/certs/ca-certificates.crt /runtime-assets/etc/ssl/certs/ \
    && cp --parents /usr/bin/curl /runtime-assets/ \
    && ldd /usr/bin/curl | awk '/=> \// {print $3}' | xargs -r -I '{}' cp -L --parents '{}' /runtime-assets/

FROM debian:bookworm-slim AS runtime
COPY --from=runtime-assets /runtime-assets/etc/ssl/certs/ /etc/ssl/certs/
COPY --from=runtime-assets /runtime-assets/usr/bin/curl /usr/bin/curl
COPY --from=runtime-assets /runtime-assets/lib/ /usr/lib/
RUN mkdir -p /var/lib/youji-worker && chown 10001:10001 /var/lib/youji-worker
COPY --from=build /build/server/target/release/youji-cloud-server /usr/local/bin/youji-cloud-server
USER 10001:10001
WORKDIR /var/lib/youji-worker
ENV YOUJI_BIND=0.0.0.0:8090
EXPOSE 8090
ENTRYPOINT ["youji-cloud-server"]
