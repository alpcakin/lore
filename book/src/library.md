# Your library

Your own commands live in one YAML file, meant to be read, edited and committed:

| Platform | Library                                            | Usage statistics                            |
|----------|-----------------------------------------------------|-----------------------------------------------|
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

The full file format, including per-shell command variants, placeholder syntax
and how to hide builtins you do not want, is documented in
[Command schema](./schema.md).

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
