# lore

A command library that lives in your shell. Press `ctrl+g`, type a few letters of
what you are trying to do, and the command lands in your prompt ready to run.

![lore in a terminal](images/demo.gif)

## The problem

The commands worth keeping are the ones you write once every few months: the
`kubectl` invocation that finds the pod that keeps restarting, the `openssl` line
that prints a certificate's expiry, the `git` incantation that finds which commit
deleted a function. Shell history keeps them for a while and then loses them
between machines, between sessions, or behind four hundred `cd ..` entries.

History is a log. What you want is a library: curated, described in words you
would actually search for, versioned in git, and open one keystroke away.

## Where to go next

- **Install lore** → [Installation](./installation.md)
- **Learn the picker and the CLI** → [Usage](./usage.md)
- **Use your library on more than one machine** → [Syncing across machines](./sync.md)
- **Understand the command file format** → [Command schema](./schema.md)
