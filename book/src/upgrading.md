# Upgrading

To see which version you have, whether there is a newer one, and what to run:

```
lore version
```

Whichever way you installed lore is the way to upgrade it:

| Installed with       | Upgrade with                                                     |
|-----------------------|--------------------------------------------------------------------|
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
