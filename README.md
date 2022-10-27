`oci-registry` is an implementation of the OCI Registry spec with filesystem and S3 storage back-ends.

[[_TOC_]]

# Features
* Pull-through cache for _any_ registry, not just docker.io
	* This includes private, authenticated registries.  **This means that you can create an unauthenticated mirror of a private registry and expose it to the Internet.  Easily.  Don't do that.**
* Two storage back-ends
	* S3
	* Local filesystem

# Limitations
* Pushing is not currently implemented; `oci-registry` only supports being a pull-through cache (a mirror) at this time.  Push support is planned.
* Authentication is not currently implemented, but is planned
* Only SHA256 content hashes are supported, but supporting other schemes is planned
* Connecting to `oci-registry` with TLS (https) is not supported and support will not be added.
	* [Using nginx as a TLS termination proxy][nginx-proxy] is easy, well-supported, and well-documented; if you require TLS between the client and `oci-registry`, that is the recommended configuration
	* Connecting to upstream registries with TLS is supported, recommended, and usually required.

# Examples
## `docker`
Mirroring `docker.io` is the default configuration.  Start `oci-registry`:
```bash
oci-registry --port 8080 filesystem --root /tmp/oci-mirror
```

Configure Docker's `daemon.json` to use the registry:
```json
{
	"registry-mirrors": ["http://localhost:8080"]
}
```

Restart Docker and it will start pulling images from `docker.io` through `oci-registry`.

**NOTE**:  Mirroring registries other than `docker.io` is not possible with Docker.

## `containerd`
`containerd` provides a mechanism for mirroring any registry you want, and sends the upstream registry as a querystring parameter in all its requests.  This means that we can mirror any number of registries to `containerd` with a single instance of `oci-registry`.

### Configure `oci-registry`
`oci-registry`'s default configuration is to mirror any registry for which it receives requests, connecting to upstream with HTTPS, rejecting invalid certs, and using the namespace as the upstream registry host - e.g. requests for `gcr.io` images will be made to https://gcr.io/ - with the exception of `docker.io`, which will be pointed to https://registry-1.docker.io

In short, `oci-registry`'s default configuration will work for most public registries, but can be added to with `--upstream-config-file`.  See [example.yaml] for real world examples, or the following contrived private registry example:
```yaml
# namespace and host are the only two required keys
- namespace: example.com
  host: registry.example.com
# Connecting with TLS is the default
  tls: true
# Requiring valid TLS certs is the default
  accept_invalid_certs: false
# This hypothetical registry checks the HTTP User-Agent header to make sure there's no malarkey going on, so pretend to be containerd
  user_agent: "containerd/1.6.8"
# This hypothetical registry requires authentication, so let's give it our username and password
  username: example
  password: hunter2
# This hypothetical registry is used for active development, so let's _always_ see if we have the latest manifest for a given image
  manifest_invalidation_time: 0s
# Blobs are identified by the SHA256 hash of their contents, so they probably won't change frequently, if ever
  blob_invalidation_time: 30d
```

### Configure `containerd`
Recent versions of `containerd` (1.5+) use [per-host configuration files][containerd-hosts]; for older versions, config instructions can be found in the deprecated section [here][containerd-deprecated].

Assuming default paths, make sure your `/etc/containerd/config.toml` contains the following:
```toml
[plugins."io.containerd.grpc.v1.cri".registry]
	config_path = "/etc/containerd/certs.d"
```

Then, in `/etc/containerd/certs.d`, create a directory for each registry you want to mirror and create a `hosts.toml` pointing at `oci-registry`:
```bash
mkdir /etc/containerd/certs.d/docker.io
cat > /etc/containerd/certs.d/docker.io/hosts.toml <<EOF
server = "https://registry-1.docker.io"

[host."http://localhost:8080"]
	capabilities = ["pull", "resolve"]
EOF

mkdir /etc/containerd/certs.d/gcr.io
cat > /etc/containerd/certs.d/gcr.io/hosts.toml <<EOF
server = "https://gcr.io"

[host."http://localhost:8080"]
	capabilities = ["pull", "resolve"]
EOF
```

The above example will configure `containerd` to attempt to pull `docker.io` and `gcr.io` manifests and blobs from `oci-registry` listening on `localhost:8080`, while sticking with the original hosts for pushing, and using the original hosts if something goes wrong with `oci-registry`.

[nginx-proxy]: https://docs.nginx.com/nginx/admin-guide/security-controls/terminating-ssl-http/
[containerd-hosts]: https://github.com/containerd/containerd/blob/main/docs/cri/config.md#registry-configuration
[containerd-deprecated]: https://github.com/containerd/containerd/blob/main/docs/cri/registry.md#configure-registry-endpoint

