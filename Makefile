pgo-data/filesystem: target/pgo-data-filesystem/release/oci-registry
target/pgo-data-filesystem/release/oci-registry:
	RUSTFLAGS=-Cprofile-generate=pgo-data/filesystem \
	RUST_LOG=info,actix-web=debug \
	RUST_BACKTRACE=1 \
	cargo run --target-dir=target/pgo-data-filesystem --release -- --port 8080 --upstream-config-file=upstream.yaml filesystem --root=test

pgo-data/s3: target/pgo-data-s3/release/oci-registry
target/pgo-data-s3/release/oci-registry:
	RUSTFLAGS=-Cprofile-generate=pgo-data/s3 \
	RUST_LOG=info,actix-web=debug \
	RUST_BACKTRACE=1 \
	cargo run --target-dir=target/pgo-data-s3 --release -- --port 8080 --upstream-config-file=upstream.yaml s3 --host=http://192.168.1.200:7480 --access-key=F504CLZ37ECLH011V4XB --secret-key=Btj2sAMtCs7GFpkmrKuMojvSdivXWt8EXy5DDZJ5 --bucket=oci-registry-test
