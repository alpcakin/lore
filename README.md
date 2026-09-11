# lore

[![CI](https://github.com/alpcakin/lore/actions/workflows/ci.yml/badge.svg)](https://github.com/alpcakin/lore/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cmdlore.svg)](https://crates.io/crates/cmdlore)
[![license](https://img.shields.io/crates/l/cmdlore.svg)](#licence)

A command library that lives in your shell. Press `ctrl+g`, type a few letters of
what you are trying to do, and the command lands in your prompt ready to run.

![lore in a terminal](docs/demo.gif)

## The problem

The commands worth keeping are the ones you write once every few months: the
`kubectl` invocation that finds the pod that keeps restarting, the `openssl` line
that prints a certificate's expiry, the `git` incantation that finds which commit
deleted a function. Shell history keeps them for a while and then loses them
between machines, between sessions, or behind four hundred `cd ..` entries.

History is a log. What you want is a library: curated, described in words you
would actually search for, versioned in git, and open one keystroke away.

## Install

Nothing here needs administrator rights. Everything installs under your home
directory. Every route that installs for you checks the archive against the
sha256 published with the release and refuses anything that does not match.

**Windows (Scoop)**

```
scoop bucket add alpcakin https://github.com/alpcakin/scoop-bucket
scoop install lore
```

**Windows (WinGet)**

```
winget install alpcakin.lore
```

**macOS and Linux (Homebrew)**

```
brew install alpcakin/tap/lore
```

**Shell installer**

```
curl -fsSL https://raw.githubusercontent.com/alpcakin/lore/main/packaging/install.sh | sh
```

```
irm https://raw.githubusercontent.com/alpcakin/lore/main/packaging/install.ps1 | iex
```

**Cargo**

```
cargo install cmdlore
```

**Prebuilt binaries**

Every release carries an archive for Windows, macOS on Intel and Apple silicon,
and Linux on x86_64 and arm64, plus a statically linked build that runs on any
Linux whatever its glibc. Download one from the
[releases page](https://github.com/alpcakin/lore/releases), check it against the
`SHA256SUMS` published beside it, unpack it, and put `lore` somewhere on your
PATH.

## Set up

```
lore setup
```

That detects your shell, shows you the single line it wants to add to your
profile, and asks before writing it. The original file is backed up first. Open a
new shell and press `ctrl+g`.

To use a different key:

```
lore setup --key alt-r
```

Write the key as `ctrl-<letter>` or `alt-<letter>`. Keys the terminal owns, such
as `ctrl-c` and `ctrl-m`, are refused with the reason.

To undo all of it:

```
lore uninstall
```

## Using it

Inside the picker:

| Key      | What it does                                        |
|----------|-----------------------------------------------------|
| type     | Filter by command, description or tags              |
| `enter`  | Put the command in your prompt                      |
| `esc`    | Close and leave the prompt alone                    |
| `ctrl+s` | Save a command you just ran                         |
| `ctrl+e` | Edit the selected entry                             |
| `ctrl+p` | Pin an entry to the top                             |
| `ctrl+x` | Remove an entry, confirmed by pressing it again     |

On the save and edit screens, `ctrl+s` submits from whichever field you are in,
so you never have to walk the rest of them to finish.

lore never runs anything. It puts the command in your prompt and pressing enter
stays your decision.

Entries frequently used rise to the top on their own. A command with one
placeholder arrives with the gap already open and the cursor in it, so you can
finish it with your shell's own completion. Commands with more placeholders ask
for the values and remember what you typed last time.

Nothing here needs the picker:

```
lore save "kubectl logs -f <pod>" --desc "Follow a pod's logs" --tags k8s,logs
lore list --shell bash
lore edit k8s.logs.follow --desc "Tail a pod"
lore rm k8s.logs.follow
```

## How the shell integration works

A keybinding has to be registered inside the shell process, so there is no way to
install one without touching a profile file. Every tool in this category works
the same way. `lore setup` appends one line between two markers:

```
# >>> lore >>>
eval "$(lore init bash)"
# <<< lore <<<
```

The snippet itself is fetched from the binary on each shell start rather than
copied into the profile, so an upgraded binary cannot disagree with a stale copy
on disk. It reads no files and does nothing but print. `lore uninstall` removes
exactly the lines between the markers and leaves everything else alone.

Your recent commands go in, and the chosen one comes back, through two temporary
files the snippet creates and deletes around each use. Neither travels on stdout:
the picker draws under your prompt, which means asking the terminal where the
cursor is, and that question goes out on stdout. A shell that captured stdout to
read the result would swallow the question, and the panel would never open.

## Your library

Your own commands live in one YAML file, meant to be read, edited and committed:

| Platform | Library                                            | Usage statistics                            |
|----------|----------------------------------------------------|---------------------------------------------|
| Linux    | `~/.config/lore/commands.yaml`                     | `~/.local/share/lore/stats.db`              |
| macOS    | `~/Library/Application Support/lore/commands.yaml` | `~/Library/Application Support/lore/stats.db` |
| Windows  | `%APPDATA%\lore\config\commands.yaml`              | `%APPDATA%\lore\data\stats.db`              |

```yaml
version: 1
commands:
  - id: k8s.logs.follow
    cmd: kubectl logs -f <pod> -n <namespace:default>
    desc: Watch what a pod is printing right now
    tags: [kubernetes, logs, debug]
    params:
      pod:
        desc: Pod name
```

Saving, editing and removing all splice single entries in and out of this file
rather than rewriting it, so your comments and your ordering survive.

Usage statistics are kept in a separate file, and on most platforms a separate
directory, because syncing your definitions through git should never produce
churn or a merge conflict. They are local to the machine and never synced.

Set `LORE_CONFIG_DIR` and `LORE_DATA_DIR` to put either somewhere else, which is
what a portable install on a stick wants.

The schema, including per shell command variants, placeholder syntax and how to
hide builtins you do not want, is documented in [docs/schema.md](docs/schema.md).

Around a hundred and fifty commands ship compiled into the binary, across git,
docker, kubernetes, ssh, networking, security tooling and more. Entries that only
make sense on a posix shell are hidden on Windows rather than offered and failing.
Anything you do not want, you can hide:

```yaml
disabled:
  - node.*
```

## Build from source

```
cargo build --release
```

Rust 1.88 or newer. `cargo fmt` and `cargo clippy --all-targets -- -D warnings`
both have to pass, and CI runs them on Linux, macOS and Windows.

## Licence

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT), at your option.
