use oci_registry::api::split_image;

fn main() {
	divan::main();
}

#[divan::bench]
fn split_image_with_ns_envoy() {
	split_image(divan::black_box(Some("docker.io")), divan::black_box("envoyproxy/envoy"), divan::black_box(""));
}

#[divan::bench]
fn split_image_with_ns_busybox() {
	split_image(divan::black_box(Some("docker.io")), divan::black_box("library/busybox"), divan::black_box(""));
}

#[divan::bench]
fn split_image_with_ns_distroless_static() {
	split_image(divan::black_box(Some("gcr.io")), divan::black_box("distroless/static"), divan::black_box(""));
}

#[divan::bench]
fn split_image_with_ns_buildbarn() {
	split_image(divan::black_box(Some("ghcr.io")), divan::black_box("buildbarn/bb-runner-installer"), divan::black_box(""));
}

#[divan::bench]
fn split_image_without_ns_envoy() {
	split_image(divan::black_box(None), divan::black_box("docker.io/envoyproxy/envoy"), divan::black_box(""));
}

#[divan::bench]
fn split_image_without_ns_busybox() {
	split_image(divan::black_box(None), divan::black_box("docker.io/library/busybox"), divan::black_box(""));
}

#[divan::bench]
fn split_image_without_ns_distroless_static() {
	split_image(divan::black_box(None), divan::black_box("gcr.io/distroless/static"), divan::black_box(""));
}

#[divan::bench]
fn split_image_without_ns_buildbarn() {
	split_image(divan::black_box(None), divan::black_box("ghcr.io/buildbarn/bb-runner-installer"), divan::black_box(""));
}

#[divan::bench]
fn split_image_without_ns_fallback_envoy() {
	split_image(divan::black_box(None), divan::black_box("envoyproxy/envoy"), divan::black_box("docker.io"));
}

#[divan::bench]
fn split_image_without_ns_fallback_busybox() {
	split_image(divan::black_box(None), divan::black_box("library/busybox"), divan::black_box("docker.io"));
}
