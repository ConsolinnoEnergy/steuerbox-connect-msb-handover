# Standalone public reference binaries; generated credentials are mounted at runtime.
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY reference ./reference
COPY spec ./spec
COPY fixtures ./fixtures
RUN cargo build --manifest-path reference/Cargo.toml --release --locked \
    -p msb -p ems-backend -p wellknown -p box-mock -p hems-mock

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --shell /usr/sbin/nologin app
COPY --from=build /src/reference/target/release/msb /usr/local/bin/msb
COPY --from=build /src/reference/target/release/ems-backend /usr/local/bin/ems-backend
COPY --from=build /src/reference/target/release/wellknown /usr/local/bin/wellknown
COPY --from=build /src/reference/target/release/box-mock /usr/local/bin/box-mock
COPY --from=build /src/reference/target/release/hems-mock /usr/local/bin/hems-mock
USER app
CMD ["msb"]
