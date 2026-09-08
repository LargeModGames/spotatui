# Release vX.Y.Z

<!-- Use with: gh pr create --template release.md -->

Version bump and changelog for vX.Y.Z. No code changes.

# Checklist

- [ ] `CHANGELOG.md`: `## [Unreleased]` renamed to `## [vX.Y.Z] YYYY-MM-DD`, every user-facing PR since the last tag has an entry
- [ ] `Cargo.toml` version bumped and `Cargo.lock` regenerated (`cargo update -p spotatui --offline`)
- [ ] `cargo metadata --locked` passes (every CI leg runs with `--locked`)

# After merge

```bash
git checkout main && git pull
git tag vX.Y.Z
git push origin vX.Y.Z
```

The tag push runs `cd.yml`. Check the release page once it finishes: the download links resolve and both `.deb` rows are present.
