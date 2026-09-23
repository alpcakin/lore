# Uninstall

Up to four things were put on your machine: a sync connection if you made one,
a line in your shell profile, the binary, and your library. Each is removed on
its own.

## 1. Sync, if you set it up

This leaves the repository and your library alone:

```
lore sync disconnect
```

## 2. The shell integration

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

## 3. The binary

Depending on how you installed it:

| Installed with       | Remove with                                                      |
|-----------------------|---------------------------------------------------------------------|
| Homebrew             | `brew uninstall lore`, then optionally `brew untap alpcakin/tap` |
| Scoop                | `scoop uninstall lore`, then optionally `scoop bucket rm alpcakin` |
| Cargo                | `cargo uninstall cmdlore`                                        |
| Shell installer, macOS and Linux | `rm ~/.local/bin/lore`                               |
| Shell installer, Windows | `Remove-Item -Recurse $env:LOCALAPPDATA\Programs\lore`, then take that folder out of your user PATH in Settings if you want it gone too |
| Prebuilt archive     | Delete `lore` from wherever you put it                           |

Removing the tap or the bucket only makes Homebrew or Scoop forget where lore
came from. Leave it if you might install again.

## 4. Your library and statistics

Only if you want them gone. They are the files listed under
[Your library](./library.md). `commands.yaml` is the library you built; keep
it if you might come back.

| Platform | Remove                                                                   |
|----------|--------------------------------------------------------------------------|
| Linux    | `rm -r ~/.config/lore ~/.local/share/lore`                               |
| macOS    | `rm -r ~/Library/Application\ Support/lore`                              |
| Windows  | `Remove-Item -Recurse $env:APPDATA\lore`                                 |
