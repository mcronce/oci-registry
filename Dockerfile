FROM rust AS builder

ARG \
	RUSTC_WRAPPER \
	RUSTFLAGS="-Cprofile-use=/repo/profile.pgodata" \
	CARGO_INCREMENTAL=1 \
	SCCACHE_ENDPOINT \
	SCCACHE_S3_USE_SSL=off \
	SCCACHE_BUCKET \
	AWS_ACCESS_KEY_ID \
	AWS_SECRET_ACCESS_KEY

RUN apt-get update && apt-get install -y cmake
ADD tools/maybe-download-sccache /
RUN /maybe-download-sccache

WORKDIR /repo
ADD profile.pgodata /repo/

COPY Cargo.toml /repo/
RUN \
	mkdir -v /repo/src && \
	echo 'fn main() {}' > /repo/src/main.rs && \
	cargo update && \
	cargo build --release && \
	bash -exc "if [ '${RUSTC_WRAPPER}' == '/sccache' ]; then /sccache -s; fi" && \
	rm -Rvf /repo/src

COPY src /repo/src

RUN \
	touch src/main.rs && \
	cargo build --release && \
	bash -exc "if [ '${RUSTC_WRAPPER}' == '/sccache' ]; then /sccache -s; fi"

RUN strip target/release/oci-registry

FROM gcr.io/distroless/cc-debian11
COPY --from=builder /repo/target/release/oci-registry /usr/local/bin/
EXPOSE 80
ENV \
	PORT=80 \
	RUST_LOG=info,actix-web=debug
ENTRYPOINT ["/usr/local/bin/oci-registry"]

