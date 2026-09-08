# Command Definition Schema (v1)

Command definitions live in plain YAML files. They are the source of truth and are
meant to be read, edited and diffed by hand. Usage statistics are never stored here;
they live in a separate local store so that syncing definitions through git does not
produce churn or merge conflicts.

## Layers

Three layers are merged at load time. Later layers win over earlier ones:

1. `builtin` - shipped with the binary, curated by the project
2. `project` - a `.lore.yml` found by walking up from the current directory
3. `user`    - the user's own library

Shadowing is by `id`. A user entry with the same `id` as a builtin replaces it
entirely. Entries can also be hidden without being redefined (see `disabled`).

Project layer files are untrusted until the user approves them once, and approval
is invalidated when the file changes.

## File structure

```yaml
version: 1
namespace: git          # optional; prefixes ids for readability, not required
commands: []            # list of entries
disabled: []            # optional; list of id glob patterns to hide
```

## Entry fields

| Field      | Required | Description                                                        |
|------------|----------|--------------------------------------------------------------------|
| `id`       | yes      | Stable unique identifier. Never derived from the command text, so that editing a command does not reset its usage history. |
| `cmd`      | yes      | Command string, or a map of shell group to command string.          |
| `desc`     | yes      | One line, imperative. Primary search surface for discovery.         |
| `tags`     | no       | List of keywords. Also searched.                                    |
| `params`   | no       | Metadata for the placeholders used in `cmd`.                        |
| `danger`   | no       | `true` marks a destructive command so the UI can warn.              |

Search matches against `cmd`, `desc` and `tags` together. Descriptions matter:
they are what makes a builtin findable by someone who knows the intent but not
the command.

## Shell variants

`cmd` may be a plain string when the command is identical everywhere, or a map
when it diverges. Recognised keys are `posix` (bash, zsh, fish) and `powershell`.
An entry with no variant for the active shell is hidden from that shell.

```yaml
cmd:
  posix: ss -tulpn
  powershell: Get-NetTCPConnection -State Listen
```

## Placeholders

Syntax is `<name>` with an optional default: `<name:default>`. Names may contain
letters, digits, `_` and `-`.

`<>` was chosen over `{{}}` because Go template syntax appears constantly in the
exact commands this tool targets (`docker ps --format '{{.Names}}'`,
`kubectl -o go-template`). A literal `<name>` is escaped as `\<name>`.

The `params` block is optional metadata. A placeholder with no entry still works,
it is simply prompted by its bare name.

```yaml
params:
  container:
    desc: Container name or ID
    from: docker ps --format "{{.Names}}"
  lines:
    desc: Number of trailing lines to show
```

`from` declares a command whose output supplies selectable values for the
placeholder. It is reserved in v1 of the schema and ignored by the MVP runtime;
entries carrying it must still work by falling back to a free text prompt.

The last value entered for a placeholder is remembered and pre-filled on the next
use, so confirming a parameterised command normally costs a single keypress.

## Disabling builtins

```yaml
disabled:
  - docker.*
  - git.clean.dry-run
```

Glob patterns matched against `id`. Lets a user silence a whole namespace they do
not use without editing shipped files.
