# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The picker mentions a newer release once there is one, naming the command
  that fits how lore was installed. It looks the release up in the background
  at most once a day, says nothing for a patch release, and never installs
  anything: lore does not replace its own binary. `LORE_NO_UPDATE_CHECK=1`
  turns it off
- `lore version` says which version you have, whether a newer one is out, and
  the one command that upgrades your kind of install
- The README says how to upgrade

### Fixed

- A change saved while a sync was already running waited for the next save or
  the picker's quarter hourly refresh to reach the repository. Syncs now queue
  instead of standing each other down


## [0.2.2] - 2026-09-20

### Changed

- Sync commits are made by `lore` rather than by the user. One commit per
  saved command, on the default branch of a repository they own, is what
  GitHub counts as a contribution, and a profile graph filling up with saved
  commands is not what anyone asked for. Setting `user.name` and `user.email`
  on lore's own clone puts their name back on them


## [0.2.1] - 2026-09-20

### Fixed

- `lore sync init` chose the address the GitHub CLI prefers, which is https
  unless told otherwise, and then failed with "could not read Username" for
  everyone whose git talks to GitHub over ssh. Both addresses are now tried,
  and when neither works the GitHub CLI is asked to give git the login it
  already holds
- A saved command's id dropped the hyphens from the words it was made of, so
  `echo from-a` became `user.echo-froma`


## [0.2.0] - 2026-09-19

### Added

- Sync across machines through a private git repository you own.
  `lore sync init` connects a machine, creating the repository with the GitHub
  CLI when it is available. After that every save, edit and removal syncs in
  the background, and the picker fetches other machines' changes when it has
  not for a while. lore merges by entry rather than by line, so two machines
  saving at once never produce a merge conflict. `lore sync`, `lore sync
  status` and `lore sync disconnect` do what they say, and
  `LORE_NO_AUTO_SYNC` turns the automatic part off

### Changed

- Saving no longer opens a form. It asks for the command and then what it is
  for, one line at a time, starting from whatever was typed at the prompt when
  the picker opened. Up and down walk the shell history. Words written as
  `#tag` become tags, and the program and its subcommands are added as tags
  without being asked for
- `lore save` asks what the command is for when `--desc` is left out, and adds
  tags from the command the same way

### Fixed

- Hiding a builtin and then saving a new command wrote the new entry into the
  list of hidden builtins, after which every lore command failed until the
  library was fixed by hand. New entries now always go into the commands list


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

[Unreleased]: https://github.com/alpcakin/lore/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/alpcakin/lore/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/alpcakin/lore/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/alpcakin/lore/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/alpcakin/lore/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/alpcakin/lore/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/alpcakin/lore/releases/tag/v0.1.0
