# Changelog

This file documents recent notable changes to this project. The format of this
file is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and
this project adheres to [Semantic
Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Tracing with a filter set by `RUST_LOG` environment variable.

### Changed

- GraphQL API `issues` and `pullRequests` return 100 items if neither `first`
  nor `last` is specified.
- GraphQL API `issues` and `pullRequests` return an error if conflicting
  pagination arguments (e.g., `first` and `before`) are provided simultaneously.

### Fixed

- Returns an error instead of an issue or pull request with "No title" as the
  title when the issue database contains an invalid key.
- No longer panics when the database contains an invalid value.

## [0.1.0] - 2022-09-06

### Added

- Initial release.

[Unreleased]: https://github.com/aicers/github-dashboard-server/compare/0.1.0...main
[0.1.0]: https://github.com/aicers/github-dashboard-server/tree/0.1.0
