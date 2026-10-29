# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.4.0 - 2026-10-29

### Added

- Check for and install signed app updates from GitHub Releases.
- Filter table rows with multiple AND/OR conditions
- Suggest values from the current page while allowing custom filter values

### Changed

- Open connections with a double-click and keep databases collapsed until selected
- Show active table filters in a compact bar and edit conditions inline
- Warn about incomplete filter conditions and explain AND/OR precedence

### Fixed

- Show connection status and all open connections in the top bar

## 0.3.0 - 2026-10-28

### Added

- Collapsible sidebar with a toolbar button and keyboard shortcut
- Connected databases at the top of the sidebar, an open-tab menu, and clearer connection context
- Keyboard shortcuts for zooming
- Create and edit supported views visually, including joins, grouped filters, HAVING and distinct aggregates

### Changed

- Make view and table actions easier to find, improve sidebar spacing, and support boolean view filters
- Larger icons in compact controls
- Improved the welcome screen, search empty states, and compact icon consistency

### Fixed

- Improved contrast for dark glass controls
- Platform-specific keyboard shortcuts

## 0.2.1 - 2026-10-28

### Fixed

- Navicat import on Windows no longer fails with 'could not save credentials' once many connections are stored
