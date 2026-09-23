# Usage

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

To take the keybinding out again, see [Uninstall](./uninstall.md).

## Inside the picker

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

## From the command line

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
