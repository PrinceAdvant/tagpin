# tagpin

A small Rust CLI and library that finds container image references that are not pinned: `FROM` lines in Dockerfiles and `image:` keys in Docker Compose files that use `latest` or no tag at all. No dependencies.

## Why

`postgres` and `postgres:latest` mean "whatever is newest when this server pulls". A redeploy months later can quietly bring a new major version, and a rollback no longer gives you the old image. tagpin lists every reference with how it is pinned, so you can fix the floating ones and enforce it in CI.

## Install

```
cargo install tagpin
```

## Usage

```
tagpin Dockerfile docker-compose.yml
tagpin --strict deploy/*.yml
```

Example output:

```
Dockerfile:1     tag       node:20-alpine
Dockerfile:9     UNTAGGED  nginx
docker-compose.yml:4     digest    postgres@sha256:6b0f...
docker-compose.yml:9     LATEST    ghcr.io/acme/worker:latest
docker-compose.yml:14    dynamic   ghcr.io/acme/app:${APP_TAG}
2 unpinned image reference(s)
```

Files whose name contains `compose` or ends in `.yml`/`.yaml` are read as Compose files; everything else as a Dockerfile. `FROM scratch` and `FROM <earlier stage>` are skipped, and `--platform` flags are ignored.

Exit code is 0 when nothing floats, 1 when at least one reference uses `latest` or no tag, and 2 for usage or read errors. With `--strict`, version tags that are not pinned by digest also fail.

As a library:

```rust
for r in tagpin::scan(&std::fs::read_to_string("compose.yml")?, "compose.yml") {
    if r.pin.is_floating() {
        eprintln!("{}:{} {}", r.file, r.line, r.image);
    }
}
```

## Limits

The scan is line based. It does not resolve Compose `extends`, YAML anchors or build arguments; references built from variables are reported as `dynamic`.

## Related

When you deploy a registry image directly, pin its tag along with its port and health check. The Peon docs on [Docker image deployments](https://peon.sh/docs/docker-images) cover that setup. Built by a contributor to [Peon](https://peon.sh/), an open-source self-hosted deployment platform.

MIT licensed.
