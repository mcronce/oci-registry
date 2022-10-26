`oci-registry` is an implementation of the OCI Registry spec with filesystem and S3 storage back-ends.

[[_TOC_]]

# Features

* Pull-through cache for _any_ registry, not just docker.io
	* This includes private, authenticated registries.  **This means that you can create an unauthenticated mirror of a private registry and expose it to the Internet.  Easily.  Don't do that.**
* Two storage back-ends
	* S3
	* Local filesystem
* Authentication

# Limitations

* Currently, `oci-registry` only supports being a pull-through cache (a mirror); pushing is not yet implemented.
* Connecting to `oci-registry` with TLS (https) is not supported and support will not be added.
	* [Using nginx as a TLS termination proxy][nginx-proxy] is easy, well-supported, and well-documented; if you require TLS between the client and `oci-registry`, that is the recommended configuration
	* Connecting to upstream registries with TLS is recommended and, typically, required.

[nginx-proxy]: https://docs.nginx.com/nginx/admin-guide/security-controls/terminating-ssl-http/

