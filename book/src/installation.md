# Installation

Nothing here needs administrator rights, and everything installs under your home
directory. Every route that installs for you checks the archive against the
sha256 published with the release and refuses anything that does not match.

## Mac or Linux, and I use Homebrew

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

## Mac or Linux, and I do not use Homebrew

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

## Windows, and I use Scoop

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

## Windows, and I do not use Scoop

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

## I have Rust

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

## I want to download it myself

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

## Troubleshooting

### `lore: command not found`

Either lore did not install, or your shell does not know where it is.

- Installed with the curl line: run `~/.local/bin/lore version`. If that
  prints a version, lore is installed and only your PATH is missing. Do steps 2
  and 3 of the curl box above. If you already did, you are in a window that was
  open before you did: run `source ~/.zshrc` or open a new one.
- Installed with Homebrew: run `brew list lore`. If it says there is no such
  formula, the install did not finish. Run `brew install alpcakin/tap/lore`
  again and read what it says.

### I pressed `ctrl+g` and nothing happens

Run `source ~/.zshrc` (on Linux with bash, `~/.bashrc`) or open a new terminal
window. `lore setup` only changes shells started after it ran. If it still does
nothing, run `lore setup` again and read what it says.

### Homebrew says my Command Line Tools are outdated

Homebrew stops until they are updated. It prints the exact command; on most
Macs it is:

```
sudo rm -rf /Library/Developer/CommandLineTools
sudo xcode-select --install
```

Then run the `brew install` line again.
