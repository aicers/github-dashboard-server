# Changelog

This file documents recent notable changes to this project. The format of this
file is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Tracing with a filter set by `RUST_LOG` environment variable.
- Added support for passing the SSH passphrase through the `SSH_PASSPHRASE`
  environment variable.
- Added new GraphQL API: `issueStat` query. Users can filter issues by
  `assignee`, `author`, `repo`(repository name), `begin` and `end` (creation
  date range). The query returns the `openIssueCount` field, indicating the
  number of open issues.
- Added additional fields to the `issues` GraphQL query, providing detailed information
  such as comments, labels, related sub-issues, linked pull requests, issue descriptions,
  timestamps, and project-related metadata.
- Exposed a new `discussions` query in the server’s GraphQL API to query the
  stored discussion data.

### Changed

- Configuration key `db_name` has been renamed to `db_path`.
- GraphQL API `issues` and `pullRequests` return 100 items if neither `first`
  nor `last` is specified.
- GraphQL API `issues` and `pullRequests` return an error if conflicting
  pagination arguments (e.g., `first` and `before`) are provided simultaneously.
- Replaced file-based config loading with the `config` crate.
- Config file must now be specified with `-c <CONFIG_PATH>`.
- `--key` and `--cert` are now required as CLI options instead of being set in
  the config file.

### Fixed

- Returns an error instead of an issue or pull request with "No title" as the
  title when the issue database contains an invalid key.
- No longer panics when the database contains an invalid value.

## [0.1.0] - 2022-09-06

### Added

- Initial release.

[Unreleased]: https://github.com/aicers/github-dashboard-server/compare/0.1.0...main
[0.1.0]: https://github.com/aicers/github-dashboard-server/tree/0.1.0
