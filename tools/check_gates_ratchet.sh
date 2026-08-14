#!/usr/bin/env bash
# Merge-base ratchet check for tools/gates.count.
#
# Usage: check_gates_ratchet.sh <base-ref>
#
# Compares the working tree's tools/gates.count against the version at the
# merge-base of <base-ref> and HEAD. Coupling counters may only fall or hold;
# test_attribute_total (the floor) may only rise or hold. The src/gates.rs
# test already pins the file to the measured values, so together the two
# checks mean a PR can neither drift from reality nor move a baseline the
# wrong way. A missing file at the merge-base is the documented bootstrap:
# the check passes with a note.
set -euo pipefail

base_ref=${1:?usage: check_gates_ratchet.sh <base-ref>}
file=tools/gates.count

merge_base=$(git merge-base "$base_ref" HEAD)

if ! old_content=$(git show "$merge_base:$file" 2>/dev/null); then
  echo "bootstrap: $file does not exist at merge-base $merge_base; nothing to compare"
  exit 0
fi

# `key = value` lines; `#` starts a comment. \r stripped for CRLF checkouts.
parse() {
  printf '%s\n' "$1" | sed 's/#.*//' |
    awk -F= 'NF == 2 { gsub(/[ \t\r]/, "", $1); gsub(/[ \t\r]/, "", $2); print $1, $2 }'
}

old_pairs=$(parse "$old_content")
new_pairs=$(parse "$(cat "$file")")

fail=0

# Reject malformed content outright instead of letting parse() drop it: after
# comment stripping, every non-blank line must be `key = <digits>`.
check_wellformed() { # $1 = label, $2 = content
  local bad
  bad=$(printf '%s\n' "$2" | sed 's/#.*//;s/\r//g' | awk 'NF' |
    grep -vE '^[ \t]*[A-Za-z0-9_]+[ \t]*=[ \t]*[0-9]+[ \t]*$' || true)
  if [ -n "$bad" ]; then
    echo "FAIL: malformed line(s) in $1:"
    printf '  %s\n' "$bad"
    fail=1
  fi
}
check_wellformed "$file" "$(cat "$file")"
check_wellformed "$file at merge-base $merge_base" "$old_content"

while read -r key old_value; do
  [ -z "$key" ] && continue
  # tail -n 1 mirrors the Rust parser's last-wins on duplicate keys.
  new_value=$(printf '%s\n' "$new_pairs" | awk -v k="$key" '$1 == k { print $2 }' | tail -n 1)
  if [ -z "$new_value" ]; then
    echo "FAIL: counter $key ($old_value at merge-base) was removed"
    fail=1
  elif ! [[ "$old_value" =~ ^[0-9]+$ && "$new_value" =~ ^[0-9]+$ ]]; then
    echo "FAIL: non-numeric value for $key (merge-base: $old_value, current: $new_value)"
    fail=1
  elif [ "$key" = "test_attribute_total" ]; then
    if [ "$new_value" -lt "$old_value" ]; then
      echo "FAIL: $key fell from $old_value (merge-base) to $new_value; the test floor may only rise"
      fail=1
    fi
  elif [ "$new_value" -gt "$old_value" ]; then
    echo "FAIL: $key rose from $old_value (merge-base) to $new_value; coupling baselines may only fall"
    fail=1
  fi
done <<<"$old_pairs"

if [ "$fail" -ne 0 ]; then
  echo "tools/gates.count moved against the ratchet relative to merge-base $merge_base"
  exit 1
fi
echo "ok: tools/gates.count respects the ratchet against merge-base $merge_base"
