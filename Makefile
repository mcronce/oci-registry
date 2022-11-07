profile.pgodata: profdata/pgo/s3 profdata/pgo/filesystem
	rm -vf profile.pgodata
	llvm-profdata merge -o profile.pgodata profdata/pgo/s3/* profdata/pgo/filesystem/*

profdata/pgo/s3: target/pgo-data-s3/release/oci-registry testdata/requests.txt
	rm -Rvf profdata/pgo/s3
	s3cmd rm -rf s3://oci-registry-test
	RUST_LOG=info,actix-web=debug \
	RUST_BACKTRACE=1 \
	./target/pgo-data-s3/release/oci-registry \
		--port 16384 \
		--upstream-config-file=upstream.yaml \
		s3 \
		--host=http://192.168.1.200:7480 \
		--access-key=F504CLZ37ECLH011V4XB \
		--secret-key=Btj2sAMtCs7GFpkmrKuMojvSdivXWt8EXy5DDZJ5 \
		--bucket=oci-registry-test \
		| sed 's/^/[s3] /' &
	sleep 0.1
	./tools/make-test-requests http://localhost:16384 | sed 's/^/[s3] /'
	echo '[s3] done requests'
	ps -ef | grep pgo-data-s3/release/oci-registry | grep -v grep | awk '{print $$2}' | xargs kill
	echo '[s3] killed oci-registry'
	sleep 0.5

target/pgo-data-s3/release/oci-registry: Cargo.toml src
	RUSTFLAGS=-Cprofile-generate=profdata/pgo/s3 \
	cargo build --target-dir=target/pgo-data-s3 --release

profdata/pgo/filesystem: target/pgo-data-filesystem/release/oci-registry testdata/requests.txt
	rm -Rvf profdata/pgo/filesystem
	rm -Rf test/*
	RUST_LOG=info,actix-web=debug \
	RUST_BACKTRACE=1 \
	./target/pgo-data-filesystem/release/oci-registry \
		--port 16385 \
		--upstream-config-file=upstream.yaml \
		filesystem \
		--root=test \
		| sed 's/^/[fs] /' &
	sleep 0.1
	./tools/make-test-requests http://localhost:16385 | sed 's/^/[fs] /'
	echo '[fs] done requests'
	ps -ef | grep pgo-data-filesystem/release/oci-registry | grep -v grep | awk '{print $$2}' | xargs kill
	echo '[fs] killed oci-registry'
	sleep 0.5

target/pgo-data-filesystem/release/oci-registry: Cargo.toml src
	RUSTFLAGS=-Cprofile-generate=profdata/pgo/filesystem \
	cargo build --target-dir=target/pgo-data-filesystem --release

