# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.7.0 - 2026-11-11

### Added

- Sort connections in the sidebar by name, last modified or last used

### Changed

- Run (⌘↵) now runs the whole script when nothing is selected, like Navicat; Run statement (⌘⇧↵) runs only the statement at the cursor

### Fixed

- Dragging a connection no longer opens a drop box below the list; drop on empty space to move it out of its folder
- Tables open right away: the total row count loads afterwards and falls back to an estimate when an exact count takes too long

## 0.6.0 - 2026-11-05

### Added

- Edit SQL directly in the visual builder, preview drafts, and apply supported changes without switching views.
- Show total matching rows for table browsing and paged query results.

### Changed

- Make visual and SQL view editor modes explicit, synchronize mode switches, and preserve table positions after SQL edits.

### Fixed

- Indent SQL in the editor with Tab and Shift+Tab.
- Preserve supported SQL edits when returning to the visual view builder.

## 0.5.0 - 2026-11-04

### Added

- Browse query results page by page without rerunning statements.
- Export SQL dumps with table and content selection and restore SQL files.
- Generate UUIDs from the context menu in new rows.
- Select all displayed result rows with Ctrl/Cmd+A and copy them.
- Show the current app version next to the name in the top bar.

### Changed

- Improve file selection and clarify backup and restore options.
- Rebrand the desktop app from guckpg to pieg while preserving saved settings and credentials.

### Fixed

- Allow importing and exporting CSV files larger than 16 MiB.
- Handle UTF-8 BOMs in CSV imports and reduce temporary files for SQL scripts.
- Warn before discarding SQL edits when returning to the visual editor for a new view.

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
