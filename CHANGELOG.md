# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2026-09-19

### Fixed

- `lore list | head`, or quitting `less` partway through the list, printed a
  panic about a broken pipe. A reader that stops early now ends the listing
  quietly, as it would for any other command line tool
- Search put `git add -A` first for "git st", because its description begins
  with "Stage". A query spelled out in the command now outranks one borrowed
  from the description, and a command that begins with the query as typed
  comes first of all

### Changed

- Ties that usage cannot break yet, which is all of them on a fresh install,
  go to the shorter command. The picker therefore opens on the basics, such as
  `df -h`, `ls -lah` and `git diff`, rather than in alphabetical order by id

## [0.1.1] - 2026-09-18

### Fixed

- The picker never opened from zsh on macOS. zsh runs a widget's commands
  with stdin on `/dev/null`, the terminal library fell back to `/dev/tty`,
  and macOS refuses to poll that device, so the picker waited forever for the
  cursor position it had asked for. The zsh snippet now hands the picker its
  terminal by name, and on macOS the binary finds its own terminal when stdin
  is not one, so bash, fish and any other caller are covered too. Upgrading
  the binary is enough: the snippet is fetched from it on every shell start
- The shell installer exited with status 1 after a successful install when the
  bin directory was not yet on PATH
- The crates.io job reported failure after a successful publish; it now checks
  the index before deciding

### Changed

- The builtin library now leads with the everyday commands, two hundred and
  forty of them across git, docker, kubernetes, ssh, networking, the file
  system, processes, text, archives, node, python and rust, with the niche
  security scanners dropped

## [0.1.0] - 2026-09-18

First release.

### Added

- A picker bound to `ctrl+g` that opens as a panel under the prompt rather than
  taking over the screen, and puts the chosen command in the prompt without ever
  running it
- Search across command text, description and tags, ranked by how often and how
  recently an entry has been used, with pinning for the ones that matter anyway
- Shell integration for bash, zsh, fish and PowerShell, installed with
  `lore setup` and removed with `lore uninstall`, both of which back up the
  profile before touching it
- A configurable keybinding: `lore setup --key alt-r`, written as
  `ctrl-<letter>` or `alt-<letter>`, with keys the terminal owns refused
- Saving from inside the picker with `ctrl+s`, offering the shell's recent
  commands, and editing with `ctrl+e`. Both screens submit on `ctrl+s` from any
  field
- Removal with `ctrl+x`, which deletes your own entry and hides a builtin
- A command line equivalent for everything but the picker: `save`, `edit`, `rm`
  and `list`
- A library of around two hundred and forty everyday commands compiled into the
  binary, across git, docker, kubernetes, ssh, networking, the file system,
  processes, text processing, archives, node, python and rust, with anything
  posix specific hidden on Windows
- A user library in one YAML file that saves, edits and removals splice entries
  in and out of, leaving comments and ordering intact
- Placeholders written `<name>` or `<name:default>`. One placeholder goes to the
  prompt with the cursor in the gap; two or more are prompted for and remembered
- `LORE_CONFIG_DIR` and `LORE_DATA_DIR` to relocate the library and the usage
  statistics, for a portable install

[Unreleased]: https://github.com/alpcakin/lore/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/alpcakin/lore/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/alpcakin/lore/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/alpcakin/lore/releases/tag/v0.1.0
