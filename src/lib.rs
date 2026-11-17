//! Find container image references that are not pinned.
//!
//! An image reference without a tag, or with the `latest` tag, can change
//! under you on the next pull. `tagpin` scans `FROM` lines in Dockerfiles and
//! `image:` keys in Docker Compose files and classifies every reference as
//! pinned by digest, pinned by tag, `latest`, untagged or dynamic (built from
//! a variable). No dependencies; the scan is line based.
//!
//! ```
//! let dockerfile = "FROM node:20-alpine AS build\nFROM nginx\nFROM build\n";
//! let refs = tagpin::scan_dockerfile(dockerfile, "Dockerfile");
//! assert_eq!(refs.len(), 2); // `FROM build` refers to a stage, not an image
//! assert_eq!(refs[0].pin, tagpin::Pin::Tag);
//! assert_eq!(refs[1].pin, tagpin::Pin::Untagged);
//!
//! let compose = "services:\n  db:\n    image: postgres@sha256:abc\n  app:\n    image: \"ghcr.io/acme/app:latest\"\n";
//! let refs = tagpin::scan_compose(compose, "compose.yml");
//! assert_eq!(refs[0].pin, tagpin::Pin::Digest);
//! assert_eq!(refs[1].pin, tagpin::Pin::Latest);
//! ```
//!
//! Pinning matters most for images you deploy straight from a registry. The
//! Peon docs on [Docker image deployments](https://peon.sh/docs/docker-images)
//! cover ports, health checks and tags for that case. Built by a contributor
//! to [Peon](https://peon.sh/), an open-source self-hosted deployment platform.

/// How an image reference is pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    /// Pinned to an immutable digest (`@sha256:...`).
    Digest,
    /// Pinned to a tag other than `latest`. Tags can be moved, but usually are not.
    Tag,
    /// Explicit `latest` tag.
    Latest,
    /// No tag at all, which means `latest`.
    Untagged,
    /// Contains a variable such as `${TAG}`; cannot be judged statically.
    Dynamic,
}

impl Pin {
    /// Short label used by the CLI.
    pub fn label(self) -> &'static str {
        match self {
            Pin::Digest => "digest",
            Pin::Tag => "tag",
            Pin::Latest => "LATEST",
            Pin::Untagged => "UNTAGGED",
            Pin::Dynamic => "dynamic",
        }
    }
    /// True for references that will float to new versions.
    pub fn is_floating(self) -> bool {
        matches!(self, Pin::Latest | Pin::Untagged)
    }
}

/// One image reference found in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub file: String,
    pub line: usize,
    pub image: String,
    pub pin: Pin,
}

/// Classify a single image reference string.
pub fn classify(image: &str) -> Pin {
    if image.contains('$') {
        return Pin::Dynamic;
    }
    if image.contains('@') {
        return Pin::Digest;
    }
    let last = image.rsplit('/').next().unwrap_or(image);
    match last.split_once(':') {
        Some((_, "latest")) => Pin::Latest,
        Some((_, t)) if !t.is_empty() => Pin::Tag,
        _ => Pin::Untagged,
    }
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    let s = s.split(" #").next().unwrap_or(s).trim();
    s.trim_matches(|c| c == '"' || c == '\'')
}

/// Scan a Dockerfile for `FROM` image references, skipping `scratch` and
/// references to earlier build stages.
pub fn scan_dockerfile(text: &str, file: &str) -> Vec<ImageRef> {
    let mut stages: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        let mut words = line.split_whitespace();
        if !words.next().is_some_and(|w| w.eq_ignore_ascii_case("FROM")) {
            continue;
        }
        let mut image: Option<&str> = None;
        let mut alias = None;
        let rest: Vec<&str> = words.filter(|w| !w.starts_with("--")).collect();
        if let Some(first) = rest.first() {
            image = Some(*first);
        }
        if rest.len() >= 3 && rest[1].eq_ignore_ascii_case("AS") {
            alias = Some(rest[2].to_ascii_lowercase());
        }
        let Some(img) = image else { continue };
        let lower = img.to_ascii_lowercase();
        let is_stage = stages.contains(&lower);
        if let Some(a) = alias {
            stages.push(a);
        }
        if lower == "scratch" || is_stage {
            continue;
        }
        out.push(ImageRef { file: file.to_string(), line: i + 1, image: img.to_string(), pin: classify(img) });
    }
    out
}

/// Scan a Docker Compose file for `image:` values.
pub fn scan_compose(text: &str, file: &str) -> Vec<ImageRef> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("image:") {
            let img = unquote(v);
            if !img.is_empty() {
                out.push(ImageRef { file: file.to_string(), line: i + 1, image: img.to_string(), pin: classify(img) });
            }
        }
    }
    out
}

/// Scan a file, choosing the parser from its name: names containing
/// `compose` or ending in `.yml`/`.yaml` are treated as Compose files,
/// everything else as a Dockerfile.
pub fn scan(text: &str, file: &str) -> Vec<ImageRef> {
    let name = file.rsplit('/').next().unwrap_or(file).to_ascii_lowercase();
    if name.contains("compose") || name.ends_with(".yml") || name.ends_with(".yaml") {
        scan_compose(text, file)
    } else {
        scan_dockerfile(text, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_refs() {
        assert_eq!(classify("nginx"), Pin::Untagged);
        assert_eq!(classify("nginx:latest"), Pin::Latest);
        assert_eq!(classify("nginx:1.27"), Pin::Tag);
        assert_eq!(classify("registry.example.com:5000/team/app"), Pin::Untagged);
        assert_eq!(classify("registry.example.com:5000/team/app:2.1"), Pin::Tag);
        assert_eq!(classify("app@sha256:0123"), Pin::Digest);
        assert_eq!(classify("app:${TAG:-dev}"), Pin::Dynamic);
    }

    #[test]
    fn dockerfile_stages_and_flags() {
        let d = "FROM --platform=$BUILDPLATFORM golang:1.22 AS build\nRUN go build\nFROM scratch\nCOPY --from=build /app /app\nFROM build AS test\nFROM alpine\n";
        let r = scan_dockerfile(d, "Dockerfile");
        let imgs: Vec<_> = r.iter().map(|x| (x.image.as_str(), x.pin, x.line)).collect();
        assert_eq!(imgs, vec![("golang:1.22", Pin::Tag, 1), ("alpine", Pin::Untagged, 6)]);
    }

    #[test]
    fn compose_values() {
        let c = "services:\n  a:\n    image: 'redis:7' # cache\n  # image: ignored\n  b:\n    build: .\n";
        let r = scan_compose(c, "compose.yaml");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].image, "redis:7");
        assert_eq!(r[0].line, 3);
    }

    #[test]
    fn picks_parser_by_name() {
        assert_eq!(scan("image: x", "docker-compose.yml").len(), 1);
        assert_eq!(scan("FROM x", "build/Dockerfile.prod").len(), 1);
    }
}
