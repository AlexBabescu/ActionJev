#!/usr/bin/env bash
# Runs only after successful tests on a trusted push to main, or manual main dispatch.
set -euo pipefail
version="$(cat VERSION)"
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Invalid VERSION' >&2; exit 1; }
major="${version%%.*}"
gh auth setup-git --hostname github.com
existing="$(git ls-remote --tags origin "refs/tags/$version")"
if [[ -n "$existing" ]]; then
  echo "$version already exists; versioned releases are never overwritten. Bump VERSION for another release."
  exit 0
fi
for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
  test -s "dist/actionjev-$target.gz"
  test -s "dist/actionjev-$target.gz.sha256"
done
cat dist/*.sha256 | LC_ALL=C sort > dist/SHA256SUMS
(cd dist; sha256sum --check SHA256SUMS)
printf '%s\n' "$GITHUB_SHA" > dist/SOURCE_COMMIT
cp VERSION dist/VERSION
# Tag-only distribution commit: source branches stay free of generated binaries.
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git add -f dist
git commit -m "Package ActionJev $version from $GITHUB_SHA"
distribution="$(git rev-parse HEAD)"
git tag -a "$version" -m "ActionJev $version" "$distribution"
git push origin "refs/tags/$version"
printf 'Portable Linux x64/ARM64 action. Only the caller-created TYPESAFE_API_KEY secret is required.\n\nSource commit: `%s`\nDistribution commit: `%s`\n\nThe tag contains checksum-verified static Rust binaries; consumers need no Cargo or Docker. Each caller supplies its own key. See README for GitHub/Gitea examples and access requirements.\n' "$GITHUB_SHA" "$distribution" > /tmp/actionjev-release-notes.md
gh release create "$version" dist/*.gz dist/SHA256SUMS dist/SOURCE_COMMIT --verify-tag --title "ActionJev $version" --notes-file /tmp/actionjev-release-notes.md
# Advance only the explicitly mutable major alias, with a lease to avoid races.
old="$(git ls-remote origin "refs/tags/$major" | awk '{print $1}')"
git tag -fa "$major" -m "ActionJev $major -> $version" "$distribution"
git push --force-with-lease="refs/tags/$major:$old" origin "refs/tags/$major"
printf 'Released %s (%s) at %s\n' "$version" "$major" "$distribution"
