# lore

[![CI](https://github.com/alpcakin/lore/actions/workflows/ci.yml/badge.svg)](https://github.com/alpcakin/lore/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cmdlore.svg)](https://crates.io/crates/cmdlore)
[![license](https://img.shields.io/crates/l/cmdlore.svg)](#licence)

A command library that lives in your shell. Press `ctrl+g`, type a few letters of
what you are trying to do, and the command lands in your prompt ready to run.

Full documentation: **https://www.alpcakin.com/lore/**

![lore in a terminal](docs/demo.gif)

## Start here

What do you want to do?

- **Install lore** → [pick your system below](#install-lore-step-by-step)
- **Use my library on a second machine or a server** → [Using lore on more than one machine](#using-lore-on-more-than-one-machine)
- **Upgrade** → [Upgrading](#upgrading)
- **Something did not work** → [Troubleshooting](#troubleshooting)
- **Remove lore** → [Uninstall](#uninstall)

### Install lore, step by step

Open the box that matches you. Every step is in it, in order.

Nothing here needs administrator rights, and everything installs under your home
directory. Every route that installs for you checks the archive against the
sha256 published with the release and refuses anything that does not match.

<details>
<summary><b>Mac or Linux, and I use Homebrew</b></summary>

1. Install it:
   ```
   brew install alpcakin/tap/lore
   ```
   If Homebrew says your Command Line Tools are outdated, it prints the command
   to fix that. Run it, then run the line above again.
2. Add the keybinding to your shell:
   ```
   lore setup
   ```
   It shows the one line it will add to your profile and asks first.
3. Load the change into this window. It only reaches shells that start after
   it, so either run this (on Linux with bash, use `~/.bashrc`):
   ```
   source ~/.zshrc
   ```
   or open a new terminal window.
4. Press `ctrl+g`, type a few letters of what you want, press `enter`.

</details>

<details>
<summary><b>Mac or Linux, and I do not use Homebrew</b></summary>

1. Install it:
   ```
   curl -fsSL https://raw.githubusercontent.com/alpcakin/lore/main/packaging/install.sh | sh
   ```
2. Tell your shell where it went. The installer puts `lore` in `~/.local/bin`,
   which a new Mac does not look in. On a Mac (zsh):
   ```
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
   ```
   On Linux with bash, use `~/.bashrc` instead of `~/.zshrc`.
3. Load that into this window, or `lore` will still be "not found" here (on
   Linux with bash, use `~/.bashrc`):
   ```
   source ~/.zshrc
   ```
4. Add the keybinding to your shell:
   ```
   lore setup
   ```
   It shows the one line it will add to your profile and asks first.
5. Load that too (`source ~/.zshrc` again, or open a new terminal window),
   then press `ctrl+g`, type a few letters of what you want, press `enter`.

</details>

<details>
<summary><b>Windows, and I use Scoop</b></summary>

1. Install it:
   ```
   scoop bucket add alpcakin https://github.com/alpcakin/scoop-bucket
   scoop install lore
   ```
2. Add the keybinding to PowerShell:
   ```
   lore setup
   ```
3. **Open a new PowerShell window**, then press `ctrl+g`.

</details>

<details>
<summary><b>Windows, and I do not use Scoop</b></summary>

1. In PowerShell, install it:
   ```
   irm https://raw.githubusercontent.com/alpcakin/lore/main/packaging/install.ps1 | iex
   ```
   It adds itself to your PATH.
2. **Open a new PowerShell window.**
3. Add the keybinding:
   ```
   lore setup
   ```
4. **Open one more new PowerShell window**, then press `ctrl+g`.

</details>

<details>
<summary><b>I have Rust</b></summary>

1. Install it:
   ```
   cargo install cmdlore
   ```
   Make sure `~/.cargo/bin` is on your PATH.
2. Add the keybinding:
   ```
   lore setup
   ```
3. Load the change into this window with `source ~/.zshrc` (on Linux with
   bash, `~/.bashrc`), or open a new terminal window.
4. Press `ctrl+g`.

</details>

<details>
<summary><b>I want to download it myself</b></summary>

1. Every release carries an archive for Windows, macOS on Intel and Apple
   silicon, and Linux on x86_64 and arm64, plus a statically linked build that
   runs on any Linux whatever its glibc. Download the one for your system from
   the [releases page](https://github.com/alpcakin/lore/releases).
2. Check it against the `SHA256SUMS` published beside it.
3. Unpack it and put `lore` in a folder that is on your PATH.
4. Add the keybinding:
   ```
   lore setup
   ```
5. Load the change into this window with `source ~/.zshrc` (on Linux with
   bash, `~/.bashrc`), or open a new terminal window, then press `ctrl+g`.

</details>

### Troubleshooting

<details>
<summary><b><code>lore: command not found</code></b></summary>

Either lore did not install, or your shell does not know where it is.

- Installed with the curl line: run `~/.local/bin/lore version`. If that
  prints a version, lore is installed and only your PATH is missing. Do steps 2
  and 3 of the curl box above. If you already did, you are in a window that was
  open before you did: run `source ~/.zshrc` or open a new one.
- Installed with Homebrew: run `brew list lore`. If it says there is no such
  formula, the install did not finish. Run `brew install alpcakin/tap/lore`
  again and read what it says.

</details>

<details>
<summary><b>I pressed <code>ctrl+g</code> and nothing happens</b></summary>

Run `source ~/.zshrc` (on Linux with bash, `~/.bashrc`) or open a new terminal
window. `lore setup` only changes shells started after it ran. If it still does
nothing, run `lore setup` again and read what it says.

</details>

<details>
<summary><b>Homebrew says my Command Line Tools are outdated</b></summary>

Homebrew stops until they are updated. It prints the exact command; on most
Macs it is:

```
sudo rm -rf /Library/Developer/CommandLineTools
sudo xcode-select --install
```

Then run the `brew install` line again.

</details>

Everything below explains how lore works and how to use it.

## The problem

The commands worth keeping are the ones you write once every few months: the
`kubectl` invocation that finds the pod that keeps restarting, the `openssl` line
that prints a certificate's expiry, the `git` incantation that finds which commit
deleted a function. Shell history keeps them for a while and then loses them
between machines, between sessions, or behind four hundred `cd ..` entries.

History is a log. What you want is a library: curated, described in words you
would actually search for, versioned in git, and open one keystroke away.

## Upgrading

To see which version you have, whether there is a newer one, and what to run:

```
lore version
```

Whichever way you installed lore is the way to upgrade it:

| Installed with       | Upgrade with                                                     |
|----------------------|------------------------------------------------------------------|
| Homebrew             | `brew upgrade lore`                                              |
| Scoop                | `scoop update lore`                                              |
| Cargo                | `cargo install cmdlore --force`                                  |
| Shell installer      | Run the same one line command again                              |
| Prebuilt archive     | Download the new one and replace the file                        |

Only the binary changes. The line in your profile asks the binary for its
integration on every shell start, so there is nothing to set up again.

lore never replaces its own binary. Once a day, when you open the picker, it
looks up the newest release in the background and mentions it the next time
you open the picker, naming the command above that fits your install. It says
so only for a new first or second number, never for a patch release. To hear
nothing at all, set `LORE_NO_UPDATE_CHECK=1`.

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

To take the keybinding out again, see [Uninstall](#uninstall).

## Using it

Inside the picker:

| Key      | What it does                                        |
|----------|-----------------------------------------------------|
| type     | Filter by command, description or tags              |
| `enter`  | Put the command in your prompt                      |
| `esc`    | Close and leave the prompt alone                    |
| `ctrl+s` | Save the command on your prompt, or one you ran     |
| `ctrl+e` | Edit the selected entry                             |
| `ctrl+p` | Pin an entry to the top                             |
| `ctrl+x` | Remove an entry, confirmed by pressing it again     |

Saving asks two things, one line at a time:

```
save: docker compose logs -f api
 for: Follow the api logs #debug
```

The command starts as whatever you had typed at the prompt, or the last command
you ran, and up and down walk back through your history the way they do in the
shell. Then say what it is for, in the words you would search with. Any word
written as `#tag` becomes a tag, and the program and its subcommands, here
`docker`, `compose` and `logs`, are added as tags on their own.

On the edit screen, `ctrl+s` saves from whichever field you are in.

You can start typing before you open it. Type `git branch`, press `ctrl+g`,
and the picker opens already searching for it. Carry on typing to narrow it
further, or press `ctrl+u` to clear and see everything. Whatever you had typed
is replaced by the command you choose, and left alone if you press `esc`.

lore never runs anything. It puts the command in your prompt and pressing enter
stays your decision.

Entries frequently used rise to the top on their own. A command with one
placeholder arrives with the gap already open and the cursor in it, so you can
finish it with your shell's own completion. Commands with more placeholders ask
for the values and remember what you typed last time.

Nothing here needs the picker. To search from the command line:

```
$ lore find docker logs
docker.logs          docker logs -f --tail <lines:100> <container>
                     Watch what a container is printing right now
docker.compose.logs  docker compose logs -f --tail <lines:100> <service>
                     Follow one service without the rest of the stack drowning it out
```

Words that are not one of lore's own commands are searched for too, so half a
remembered command is answered rather than refused:

```
lore docker logs
```

Every word has to appear somewhere in the command, its description or its
tags, so `docker log` finds `docker logs`. Nothing is guessed: a typo matches
nothing and says so.

A broad search stops after ten matches and says how many more there are, so a
whole namespace never rolls past your prompt. `--all` prints every one, and so
does piping the output into something else.

`-1` prints the best match's command and nothing else, for use in a pipeline
or inside another command:

```
$ lore find -1 curl timing
curl -w "dns %{time_namelookup}s ..." -o /dev/null -s <url>
```

The rest of the library is managed the same way:

```
lore save "kubectl logs -f <pod>" --desc "Follow a pod's logs #k8s"
lore save "terraform plan"          # asks what it is for
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

The answer comes back on the terminal's input, so the picker needs that on
stdin too. zsh runs a widget's commands with stdin closed off, so its snippet
hands the picker the terminal by name, and on macOS the binary can find its own
terminal when it has to.

Upgrading the binary is enough for most changes, since the snippet is fetched
from it on every shell start. When the release notes say to run `lore setup`
again, the one line in the profile itself changed.

## Using lore on more than one machine

Your library can follow you to a second laptop, a work machine or a server,
through a private git repository that you own. Nothing is sent anywhere else,
and this is optional: lore works fully on one machine without it.

With the [GitHub CLI](https://cli.github.com) installed and logged in, run this
on each machine:

```
lore sync init
```

The first time, it creates a private repository called `lore-library` on your
GitHub account. On every other machine it finds that repository and connects to
it. Without the GitHub CLI, create an empty private repository on any git host
and give its address instead:

```
lore sync init git@github.com:you/lore-library.git
```

After that there is nothing to remember. Every save, edit and removal is synced
in the background, and the picker fetches other machines' changes when it has
not done so for a while. To sync right away, or to see why a sync failed:

```
lore sync
lore sync status
```

lore runs your own `git`, so it uses whatever login `git push` already uses, and
never sees a password or token. On a server, that means the server needs access
to the repository, for example through an SSH key.

Two machines can save at the same time without a merge conflict. lore merges by
entry rather than by line, so new commands from both are kept. If the same
entry was edited differently on two machines, the version synced first keeps
its id and the other is kept as a copy, such as `user.deploy-2`, and the sync
says so.

Only the library is synced. Which commands you use most is kept per machine.

Sync commits are made by `lore`, not by you, so saving a command does not file
a contribution against your GitHub profile. To put your own name on them
instead, set an identity on lore's clone of the repository:

```
git -C ~/.local/share/lore/sync config user.name "Your Name"
git -C ~/.local/share/lore/sync config user.email "you@example.com"
```

On macOS that directory is `~/Library/Application Support/lore/sync`, and on
Windows `%APPDATA%\lore\data\sync`.

Keep the repository private. Saved commands often contain server names, user
names and internal addresses.

To stop syncing on a machine, run `lore sync disconnect`. Your library stays,
and so does the repository. Set `LORE_NO_AUTO_SYNC=1` to sync only when you run
`lore sync` yourself.

If you would rather not use git at all, point `LORE_CONFIG_DIR` at a folder
that Dropbox, iCloud Drive or OneDrive already syncs.

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

Around two hundred and forty commands ship compiled into the binary: the everyday
git, docker, kubernetes, ssh, networking, file system, process, text, archive,
node, python and rust commands, each described by what it is for rather than
by its flags. Entries that only make sense on a posix shell are hidden on
Windows rather than offered and failing. Anything you do not want, you can
hide:

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

## Uninstall

Up to four things were put on your machine: a sync connection if you made one,
a line in your shell profile, the binary, and your library. Each is removed on
its own.

**1. Sync**, if you set it up. This leaves the repository and your library
alone:

```
lore sync disconnect
```

**2. The shell integration**

```
lore uninstall
```

This removes exactly the lines between the `# >>> lore >>>` and `# <<< lore <<<`
markers from your profile and leaves everything else alone. The profile is
backed up first, beside the original with `.lore-backup` added to its name,
such as `.zshrc.lore-backup`. Delete the backup once you are happy.

If you set up more than one shell, run it once per shell with `--shell bash`,
`--shell zsh`, `--shell fish` or `--shell powershell`. Do this before removing
the binary, since it is the binary that knows where the profile is.

**3. The binary**, depending on how you installed it:

| Installed with       | Remove with                                                      |
|----------------------|------------------------------------------------------------------|
| Homebrew             | `brew uninstall lore`, then optionally `brew untap alpcakin/tap` |
| Scoop                | `scoop uninstall lore`, then optionally `scoop bucket rm alpcakin` |
| Cargo                | `cargo uninstall cmdlore`                                        |
| Shell installer, macOS and Linux | `rm ~/.local/bin/lore`                               |
| Shell installer, Windows | `Remove-Item -Recurse $env:LOCALAPPDATA\Programs\lore`, then take that folder out of your user PATH in Settings if you want it gone too |
| Prebuilt archive     | Delete `lore` from wherever you put it                           |

Removing the tap or the bucket only makes Homebrew or Scoop forget where lore
came from. Leave it if you might install again.

**4. Your library and statistics**, only if you want them gone. They are the
files listed under [Your library](#your-library). `commands.yaml` is the
library you built; keep it if you might come back.

| Platform | Remove                                                                   |
|----------|--------------------------------------------------------------------------|
| Linux    | `rm -r ~/.config/lore ~/.local/share/lore`                               |
| macOS    | `rm -r ~/Library/Application\ Support/lore`                              |
| Windows  | `Remove-Item -Recurse $env:APPDATA\lore`                                 |

## Contributing

Bug reports, suggested commands and pull requests are welcome. See
[CONTRIBUTING.md](CONTRIBUTING.md) for how to build and test, and what makes a
good builtin command.

## Licence

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT), at your option.
