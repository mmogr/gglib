# Badges Branch

This branch is automatically managed by GitHub Actions and serves badge JSON files for the repository.

## Purpose

The badges branch hosts JSON files that are used by [Shields.io](https://shields.io/) to display dynamic badges in the main repository README. These badges show real-time information about:

- Test results (Rust and TypeScript)
- Code coverage metrics
- Lines of code and complexity
- Version information
- Architectural boundary checks

## Automated Updates

Badge files are automatically generated and updated by the following GitHub Actions workflows:

- **CI Workflow**: Updates test badges and boundary check badges
- **Coverage Workflow**: Updates code coverage badges
- **Release Workflow**: Updates version and complexity metrics

## Structure

Badge files follow the naming convention:
- `{crate-name}-{metric}.json` for crate-level metrics
- `{crate-name}-{module}-{metric}.json` for module-level metrics
- `ts-{metric}.json` for TypeScript metrics
- Special badges: `tests.json`, `coverage.json`, `boundary.json`, `version.json`

## Do Not Edit Manually

All files in this branch are automatically generated. Manual edits will be overwritten by the next workflow run.
