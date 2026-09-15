# Docker builds

`docker/Dockerfile` builds the application image (target `runtime`) and the
test image (target `test-runtime`) for `linux/amd64`, `linux/arm64`, and
`linux/arm/v7`.

Build and smoke-test from the repository root. Change the platform and tags as
needed:

```sh
docker buildx build --platform linux/arm/v7 --target runtime \
    -f docker/Dockerfile -t next:local-armv7 --load .
docker buildx build --platform linux/arm/v7 --target test-runtime \
    -f docker/Dockerfile -t next-test:local-armv7 --load .
sh docker/smoke-test.sh linux/arm/v7 next:local-armv7 next-test:local-armv7
```

Rust is compiled on the build host. A cross compiler is used when the target
architecture is different. Only the runtime stages and the smoke test run under
QEMU, so the Docker host needs QEMU binfmt support for foreign platforms.

The GitHub workflow builds both targets in one job per architecture, so the
test build reuses the toolchain and dependency layers. The smoke test does not
need GPU access.
