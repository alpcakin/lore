# Syncing across machines

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
