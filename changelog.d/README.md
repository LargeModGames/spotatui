# Changelog fragments

Each change that users notice adds one file here instead of a line in
`CHANGELOG.md`, so two open PRs never conflict over the changelog.

- Path: `changelog.d/<section>/<issue>-<short-name>.md`, for example
  `changelog.d/fixed/1016-youtube-view-counts.md`. Without an issue, use the
  short name alone.
- Sections: `added`, `changed`, `removed`, `fixed`, `security`, `performance`,
  `documentation`, `internal`.
- Content: the entry exactly as it will appear in `CHANGELOG.md`, one bullet:

  ```markdown
  - **YouTube view counts just below the next unit round up to that unit**: 999,999 views showed as `1000.0K views`. They now show as `1.0M views` ([#1016](https://github.com/LargeModGames/spotatui/issues/1016)).
  ```

At release, `tools/assemble_changelog.sh vX.Y.Z` moves every fragment into a new
section of `CHANGELOG.md` and deletes the files.
