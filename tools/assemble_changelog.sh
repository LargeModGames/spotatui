#!/usr/bin/env bash
# Moves the changelog.d fragments and any entries under `## [Unreleased]` into a
# new `## [<version>] <today>` section of CHANGELOG.md, then deletes the fragments.
# Usage: tools/assemble_changelog.sh v0.49.0
set -euo pipefail
shopt -s nullglob

version=${1:?usage: tools/assemble_changelog.sh vX.Y.Z}
cd "$(git rev-parse --show-toplevel)"

sections="added changed removed fixed security performance documentation internal"

for dir in changelog.d/*/; do
  case " $sections " in
    *" $(basename "$dir") "*) ;;
    *) echo "unknown changelog section: $dir" >&2; exit 1 ;;
  esac
done

changelog=$(tr -d '\r' < CHANGELOG.md)
unreleased=$(printf '%s\n' "$changelog" | awk '/^## \[Unreleased\]/ { f = 1; next } /^## \[/ { f = 0 } f')

while IFS= read -r heading; do
  title=${heading#\#\#\# }
  case " $sections " in
    *" ${title,,} "*) ;;
    *) echo "unknown heading under Unreleased: $heading" >&2; exit 1 ;;
  esac
done < <(printf '%s\n' "$unreleased" | grep '^### ' || true)

release=""
for dir in $sections; do
  title=${dir^}
  existing=$(printf '%s\n' "$unreleased" | awk -v t="### $title" '$0 == t { f = 1; next } /^### / { f = 0 } f')
  entries=$( { awk NF /dev/null changelog.d/"$dir"/*.md; printf '%s\n' "$existing"; } | tr -d '\r' | awk NF)
  [ -n "$entries" ] && release+=$'\n'"### $title"$'\n\n'"$entries"$'\n'
done

if [ -z "$release" ]; then
  echo "nothing to release: no fragments and no Unreleased entries" >&2
  exit 1
fi

{
  printf '%s\n' "$changelog" | awk '!done { print } /^## \[Unreleased\]/ { done = 1 }'
  printf '\n## [%s] %s\n%s\n' "$version" "$(date +%F)" "$release"
  printf '%s\n' "$changelog" | awk '/^## \[Unreleased\]/ { s = 1; next } s && /^## \[/ { f = 1 } f'
} > CHANGELOG.md.tmp
mv CHANGELOG.md.tmp CHANGELOG.md

git rm -q --ignore-unmatch -- 'changelog.d/*/*.md'
echo "CHANGELOG.md: added ## [$version]"
