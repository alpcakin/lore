# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

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
  commands, and editing with `ctrl+e`
- Removal with `ctrl+x`, which deletes your own entry and hides a builtin
- A command line equivalent for everything but the picker: `save`, `edit`, `rm`
  and `list`
- A library of around a hundred and fifty commands compiled into the binary,
  across git, docker, kubernetes, ssh, networking, security tooling, archives,
  text processing, rust and node, with anything posix specific hidden on Windows
- A user library in one YAML file that saves, edits and removals splice entries
  in and out of, leaving comments and ordering intact
- Placeholders written `<name>` or `<name:default>`. One placeholder goes to the
  prompt with the cursor in the gap; two or more are prompted for and remembered
- `LORE_CONFIG_DIR` and `LORE_DATA_DIR` to relocate the library and the usage
  statistics, for a portable install

[Unreleased]: https://github.com/alpcakin/lore/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/alpcakin/lore/releases/tag/v0.1.0
