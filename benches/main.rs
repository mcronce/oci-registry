use criterion::criterion_group;
use criterion::criterion_main;
use criterion::BenchmarkId;
use criterion::Criterion;

fn split_image_with_ns(c: &mut Criterion) {
	c.bench_with_input(BenchmarkId::new("split_image", "with ns docker.io/envoyproxy/envoy"), &(Some("docker.io"), "envoyproxy/envoy", ""), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});

	c.bench_with_input(BenchmarkId::new("split_image", "with ns docker.io/library/busybox"), &(Some("docker.io"), "library/busybox", ""), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});

	c.bench_with_input(BenchmarkId::new("split_image", "with ns gcr.io/distroless/static"), &(Some("gcr.io"), "distroless/static", ""), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});

	c.bench_with_input(
		BenchmarkId::new("split_image", "with ns ghcr.io/buildbarn/bb-runner-installer"),
		&(Some("ghcr.io"), "buildbarn/bb-runner-installer", ""),
		|b, &(ns, image, default_ns)| b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	);
}

fn split_image_without_ns(c: &mut Criterion) {
	c.bench_with_input(BenchmarkId::new("split_image", "without ns docker.io/envoyproxy/envoy"), &(None, "docker.io/envoyproxy/envoy", ""), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});

	c.bench_with_input(BenchmarkId::new("split_image", "without ns docker.io/library/busybox"), &(None, "docker.io/library/busybox", ""), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});

	c.bench_with_input(BenchmarkId::new("split_image", "without ns gcr.io/distroless/static"), &(None, "gcr.io/distroless/static", ""), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});

	c.bench_with_input(
		BenchmarkId::new("split_image", "without ns ghcr.io/buildbarn/bb-runner-installer"),
		&(None, "ghcr.io/buildbarn/bb-runner-installer", ""),
		|b, &(ns, image, default_ns)| b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	);
}

fn split_image_fallback(c: &mut Criterion) {
	c.bench_with_input(BenchmarkId::new("split_image", "fallback docker.io/envoyproxy/envoy"), &(None, "envoyproxy/envoy", "docker.io"), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});

	c.bench_with_input(BenchmarkId::new("split_image", "fallback docker.io/library/busybox"), &(None, "library/busybox", "docker.io"), |b, &(ns, image, default_ns)| {
		b.iter(|| oci_registry::api::split_image(ns, image, default_ns))
	});
}

criterion_group!(split_image, split_image_with_ns, split_image_without_ns, split_image_fallback);
criterion_main!(split_image);
