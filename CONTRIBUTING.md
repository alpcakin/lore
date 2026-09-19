# Contributing to lore

Thank you for taking the time. Bug reports, new builtin commands and fixes are
all welcome. This page covers what you need to get a change merged.

## Reporting a bug

Open an issue with the **Bug report** form. The two things that make a report
actionable are the output of `lore --version` and which shell and terminal you
were in: most bugs in a tool like this depend on the shell, and several have
depended on the terminal.

If the picker does not open at all, say what you saw on screen, even if it was
only the prompt moving down a line.

## Suggesting a command

Open an issue with the **Suggest a command** form, or send a pull request
straight away. The library is meant to hold the commands people reach for
every week, not every flag a tool has. A good candidate is one you have looked
up more than once.

## Building

You need Rust 1.88 or newer.

```
cargo build
cargo test
```

Before pushing, run what CI runs. A pull request that fails any of these will
not be merged.

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

CI runs them on Linux, macOS and Windows, and checks the minimum Rust version
separately.

## Adding a builtin command

Builtins live in `assets/builtins`, one YAML file per tool. Add yours to the
file for its tool, next to the entries it is closest to. The format is
documented in [docs/schema.md](docs/schema.md).

```yaml
  - id: git.stash.pop
    cmd: git stash pop
    desc: Bring back the changes you last put aside
    tags: [git, stash, pop, restore]
```

The tests check most of the following, and review checks the rest.

- **The id** is the namespace of its file, a dot, and a short lowercase name.
  Only letters, digits, dots and hyphens.
- **The description** says what the command is for, in the words someone
  would search with, rather than restating its flags. "See what has changed" is
  findable; "Show the working tree status" is only findable by someone who
  already knows the command.
- **At least two tags**, chosen for what people type when they cannot remember
  the name.
- **Every placeholder has a description** under `params`. Use `<name:default>`
  when there is a sensible default.
- **A `powershell` variant** when the posix command does not exist on Windows.
  An entry with no variant for a shell is hidden from it, which is better than
  offering a command that fails.
- **`danger: true`** on anything that deletes data or cannot be undone. The
  picker marks it in red.
- **ASCII only.** A legacy Windows console renders anything else as mojibake.

Check the posix command on both Linux and macOS. The BSD tools on macOS reject
several GNU flags, such as `ps --sort` and `du --max-depth`.

## Changing the picker or the shell integration

The picker and the four shell snippets are the parts unit tests reach least.
Try a change in a real terminal with each shell it affects:

```
cargo build --release
eval "$(./target/release/lore init zsh)"
```

Then press `ctrl+g`. For PowerShell, use
`Invoke-Expression (& .\target\release\lore.exe init powershell | Out-String)`.

## Commits and pull requests

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/),
with the area in brackets: `fix(search): ...`, `feat(builtins): ...`,
`docs: ...`. The subject says what changes for the user. The body, when there
is one, says why.

Keep a pull request to one change. Add a line under `Unreleased` in
[CHANGELOG.md](CHANGELOG.md) when users will notice it.

## Licence

By contributing, you agree that your contribution is licensed under both the
MIT and Apache 2.0 licences, as the rest of the project is.
