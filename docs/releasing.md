# Release process

[Back to README](../README.md)

## Publishing (maintainers)

1. Set the GitHub Actions secret `CARGO_REGISTRY_TOKEN` to a crates.io API token
   with permission to publish `twokey`. Store it as a repository secret or in
   the `crates-io` environment used by the publishing job.
2. Make sure `version` in `Cargo.toml` matches the tag you'll push, and that
   `Cargo.lock` is committed.
3. Push the tag (`git tag v0.2.0 && git push origin v0.2.0`). The release
   workflow tests and builds every target, vendors the source and proves it
   builds offline, writes `SHA256SUMS`, attests provenance, and creates the
   GitHub release, then runs `cargo publish --locked` using the token. This also
   supports the first publication; no manual publish or Trusted Publishing
   configuration is required. A missing token fails the job with a clear error.

## Distribution and verification

The release workflow is configured to publish prebuilt binaries (static Linux musl for x86_64
and aarch64, macOS arm64 and x86_64, Windows x86_64), a **vendored source bundle
that builds with no network access**, `SHA256SUMS`, and build-provenance
attestations. These commands require an existing release or crates.io publication.

| How | Command |
|---|---|
| Script (Linux/macOS) | `curl -fsSL https://github.com/Azazel-Labs/twokey/releases/download/v0.2.0/install.sh \| sh -s -- --version v0.2.0` |
| Pinned checksum | add `--sha256 <hex>` (from the release's `SHA256SUMS`) |
| Script (Windows PowerShell) | Download release asset `install.ps1`, then `./install.ps1 -Version v0.2.0` (see [README](../README.md#installation)) |
| Windows pinned checksum | add `-Sha256 <hex>`; use `-To <directory>` for a custom destination |
| GitHub Actions | `uses: Azazel-Labs/twokey@<commit-sha>` with `version:` and `sha256:` (see [action.yml](../action.yml)) |
| Direct download | `https://github.com/Azazel-Labs/twokey/releases/download/<tag>/twokey-<tag>-<target>.tar.gz` |
| Offline from source | download `twokey-<tag>-vendored-src.tar.gz`, then `cargo install --path <dir> --offline --locked` (or `install.sh --source`) |
| cargo-binstall | `cargo binstall twokey` (fetches the GitHub binary) |
| crates.io | `cargo install twokey --locked` |

Verify a download yourself with `sha256sum -c SHA256SUMS --ignore-missing` and
`gh attestation verify <file> --repo Azazel-Labs/twokey`. In CI, always pin both
the version and the sha256.
