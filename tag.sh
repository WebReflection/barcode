#!/usr/bin/env bash
#
# tag.sh — tag the published web package version once the version bump AND
# the generated docs are pushed.
#
# Why this exists: `npm version patch` only commits and tags when the
# package's `package.json` sits at the repository root. The published
# package lives in `packages/web/`, so npm bumps the file and silently skips
# both the commit and the tag. This script performs the tag step explicitly.
#
#   ./tag.sh             # tag v<version> at HEAD and push it
#   ./tag.sh --dry-run   # run every check, print what would happen
#
# It refuses to run unless:
#   - the working tree is clean (docs rebuild committed),
#   - HEAD is on a branch and equal to its origin counterpart (docs pushed),
#   - the tag does not already exist locally or on origin.
#
set -euo pipefail

cd "$(dirname "$0")"

die() {
  printf 'tag.sh: %s\n' "$*" >&2
  exit 1
}

dry_run=0
if [ "${1:-}" = "--dry-run" ]; then
  dry_run=1
elif [ -n "${1:-}" ]; then
  die "unknown argument: $1 (use --dry-run)"
fi

pkg="packages/web/package.json"
[ -f "$pkg" ] || die "missing $pkg"
version=$(node -p "require('./$pkg').version")
[ -n "$version" ] || die "could not read the version from $pkg"
tag="v$version"

if [ -n "$(git status --porcelain)" ]; then
  git status --short >&2
  die "working tree is dirty — commit and push the docs rebuild first"
fi

branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" != "HEAD" ] || die "detached HEAD — check out the release branch first"

git fetch --quiet origin
head=$(git rev-parse HEAD)
remote=$(git rev-parse --verify -q "origin/$branch" || true)
[ -n "$remote" ] || die "origin/$branch does not exist — push the branch first"
[ "$head" = "$remote" ] ||
  die "HEAD ($head) is not pushed to origin/$branch ($remote) — push the docs commit first"

git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "$tag already exists locally"
git ls-remote --quiet --tags origin "refs/tags/$tag" | grep -q . && die "$tag already exists on origin"

if [ "$dry_run" = 1 ]; then
  printf 'tag.sh: would tag %s at %s (%s)\n' \
    "$tag" "$(git rev-parse --short HEAD)" "$(git show -s --format=%s HEAD)"
  exit 0
fi

git tag -a "$tag" -m "$version"
git push origin "refs/tags/$tag"
printf 'tag.sh: %s -> %s (%s)\n' \
  "$tag" "$(git rev-parse --short HEAD)" "$(git show -s --format=%s HEAD)"
