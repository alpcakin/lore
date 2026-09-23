# How the shell integration works

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
