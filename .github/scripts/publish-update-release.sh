#!/usr/bin/env bash
set -euo pipefail
release_dir="${1:-release}"
: "${RELEASE_TAG:?缺少发布标签}"
: "${RELEASE_SHA:?缺少发布提交}"
: "${GITHUB_REPOSITORY:?缺少仓库标识}"
shopt -s nullglob
installers=("$release_dir"/*.exe)
if [[ ${#installers[@]} != 1 || ! -f "${installers[0]}.sig" || ! -f "$release_dir/latest.json" || ! -f "$release_dir/release-notes.md" ]]; then
  echo "签名发布产物不完整，停止发布。" >&2
  exit 1
fi
if tag_sha=$(git rev-parse --verify "refs/tags/$RELEASE_TAG^{commit}" 2>/dev/null); then
  if [[ "$tag_sha" != "$RELEASE_SHA" ]]; then
    echo "版本标签已指向其他提交，请升级版本号后发布。" >&2
    exit 1
  fi
fi
flags=()
if [[ "$RELEASE_TAG" == *-* ]]; then
  flags+=(--prerelease)
fi
if draft=$(gh release view "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --json isDraft --jq '.isDraft'); then
  if [[ "$draft" != "true" ]]; then
    echo "Release 已发布，跳过重复发布。"
    exit 0
  fi
else
  gh release create "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" \
    --target "$RELEASE_SHA" --title "游迹 $RELEASE_TAG" --draft \
    --notes-file "$release_dir/release-notes.md" "${flags[@]}"
fi
gh release upload "$RELEASE_TAG" "${installers[0]}" "${installers[0]}.sig" "$release_dir/latest.json" \
  --repo "$GITHUB_REPOSITORY" --clobber
latest=(--latest)
publish_kind=(--prerelease=false)
if [[ "$RELEASE_TAG" == *-* ]]; then
  latest=(--latest=false)
  publish_kind=(--prerelease)
else
  stable_tags=$(gh api "repos/$GITHUB_REPOSITORY/releases" --paginate --jq '.[] | select(.draft == false and .prerelease == false) | .tag_name')
  newest=$(printf '%s\n' "$stable_tags" "$RELEASE_TAG" | sort -V | tail -n 1)
  if [[ "$newest" != "$RELEASE_TAG" ]]; then
    latest=(--latest=false)
  fi
fi
gh release edit "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" \
  --notes-file "$release_dir/release-notes.md" --draft=false "${latest[@]}" "${publish_kind[@]}"
