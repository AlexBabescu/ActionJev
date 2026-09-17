# Distribution and releases

## Consumer contract

The published `v0` and `v0.1.0` tags contain compressed, statically linked Rust executables for Linux x64 and ARM64. `scripts/prepare.sh` selects the architecture, verifies the archive against the **committed** `dist/SHA256SUMS`, and decompresses it into a fresh runner-temporary directory. It never downloads or compiles anything by default. Source branches do not carry generated binaries: use a release tag, a distribution commit SHA, `binary-path`, or explicit `build-from-source: true`.

Runtime requirements: Linux x64/ARM64, Bash, Git, gzip, awk and sha256sum. A separate checkout action may need Node. ActionJev needs neither a Rust toolchain, a Docker socket, a GitHub release-download token, nor a registry login. The bundled static binary also avoids a glibc-version dependency inside job containers.

`typesafe-api-key` must be explicitly passed from the **calling repository's** secrets. Merely naming a secret does not make it visible to an imported action. ActionJev's own repository secret is used only for its maintainer integration test and is never packaged, forwarded to consumers, or used to pay for their reviews. PR comments use the automatically supplied job token by default. Gitea administrators may clamp permissions; set `comment: 'false'` for report-only use when writes are unavailable, or supply an explicitly authorized token.

## Release process

CI tests both native Linux architectures, builds musl binaries with the committed Cargo lockfile, runs offline integration/launcher tests and both composite execution paths, then performs a three-request live test against a tiny synthetic fixture on trusted non-PR runs. This fixture is the only code sent by the maintainer live test. Future trusted CI runs consume a small number of TypeSafe tokens; PR runs remain fully offline.

On `main`, after all prerequisites pass, `scripts/release.sh` packages a **tag-only distribution commit** parented to the tested source commit. It records `dist/SOURCE_COMMIT`, generates checksums, pushes a new immutable version tag, uploads release assets, and advances the floating major alias. Source branch history remains free of generated binary blobs. Version tags are never overwritten. The major alias is advanced with a Git lease to detect concurrent updates.

For another release: update `VERSION` to the next semantic version, open a PR and merge after review. The trusted `main` workflow publishes it. Existing versions are skipped. `workflow_dispatch` on `main` can retry a failed run. A partial publish after pushing an immutable tag requires manual recovery, not overwriting that tag: inspect the tag/assets and finish the missing release/major-alias steps. `scripts/release.sh` does not silently replace an existing version.

A floating `@v0` provides compatible updates. Pin the release's **distribution commit SHA**, not its source commit SHA, for immutable action code, prompts, and binary digests. This protects against a subsequently moved tag; checksums alone are not an independent signature or proof of a trusted build.

## GitHub visibility and Gitea mirrors

An action repository must be public for arbitrary unrelated users to import it from GitHub without additional access arrangements. This project does not change repository visibility automatically. Private-action sharing is restricted by GitHub's access settings; it is not equivalent to public distribution. Publishing a release does not make a private repository public.

Gitea can use `uses: https://github.com/AlexBabescu/ActionJev@v0` once the repository is public. Otherwise mirror it to an accessible Gitea repository. **Mirror tags and their reachable objects, not only main:** the binaries live in the distribution tags. No release assets or GitHub access tokens are needed at action runtime once that tagged repository is accessible. The built-in Gitea job token remains scoped to the caller's instance/repository, not GitHub.

Sources: [GitHub action releases](https://docs.github.com/en/actions/how-tos/create-and-publish-actions/release-and-maintain-actions), [private action sharing](https://docs.github.com/en/actions/how-tos/reuse-automations/share-across-private-repositories), [Gitea absolute action URLs](https://docs.gitea.com/usage/actions/comparison/), [Gitea job-token permissions](https://docs.gitea.com/usage/actions/token-permissions/).
