FROM mcronce/rust-bolt AS builder

ARG \
	RUSTC_WRAPPER \
	SCCACHE_ENDPOINT \
	SCCACHE_S3_USE_SSL=off \
	SCCACHE_BUCKET \
	AWS_ACCESS_KEY_ID \
	AWS_SECRET_ACCESS_KEY

RUN apt-get update && apt-get install -y cmake s3cmd

WORKDIR /repo

COPY Cargo.toml /repo/
RUN \
	mkdir -v /repo/src && \
	echo 'fn main() {}' > /repo/src/main.rs && \
	cargo update && \
	cargo pgo build && \
	bash -exc "if [ '${RUSTC_WRAPPER}' == '/sccache' ]; then /sccache -s; fi" && \
	rm -Rvf /repo/src

COPY src /repo/src

RUN \
	touch src/main.rs && \
	cargo pgo build && \
	bash -exc "if [ '${RUSTC_WRAPPER}' == '/sccache' ]; then /sccache -s; fi"

ADD tools /repo/tools
ADD testdata /repo/testdata

RUN mv -vf /repo/testdata/s3cfg ~/.s3cfg
RUN \
	export LLVM_PROFILE_FILE=/repo/target/pgo-profiles/oci-registry_%m_%p.profraw && \
	./tools/generate-profiles '' | sed 's/^/[ pgo] /'

RUN \
	cargo pgo bolt build --with-pgo && \
	./tools/generate-profiles -bolt-instrumented | sed 's/^/[bolt] /'

RUN cargo pgo bolt optimize --with-pgo
RUN strip /repo/target/x86_64-unknown-linux-gnu/release/oci-registry-bolt-optimized

FROM gcr.io/distroless/cc-debian11
COPY --from=builder /repo/target/x86_64-unknown-linux-gnu/release/oci-registry-bolt-optimized /usr/local/bin/oci-registry
EXPOSE 80
ENV \
	PORT=80 \
	RUST_LOG=info,actix-web=debug
ENTRYPOINT ["/usr/local/bin/oci-registry"]

