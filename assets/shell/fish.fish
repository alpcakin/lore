function __lore_pick
    # Newest first, in a file rather than in arguments: Windows rebuilds a
    # child's argument list out of one string, so a command ending in a
    # backslash takes the next one with it. Every shell hands it over the same
    # way.
    set -l recent (mktemp)
    set -l out (mktemp)
    history --max 50 > $recent

    # Whatever is already on the prompt opens the picker as a search, and is
    # the first thing offered when saving.
    set -l line (mktemp)
    printf '%s' (commandline) > $line

    # Deliberately not a command substitution. The picker draws a panel under
    # the prompt, which means asking the terminal where the cursor is, and that
    # question goes out on stdout. A captured stdout swallows it, no answer
    # comes back, and the panel never opens. The result comes back in a file so
    # stdout can stay attached to the terminal.
    lore pick --shell fish --print-cursor --history $recent --line $line --output $out

    # The offset is first on a line of its own, so the command is the rest.
    set -l chosen (cat $out)
    rm -f $recent $line $out

    if test (count $chosen) -ge 2
        commandline -r -- (string join \n $chosen[2..-1])
        commandline -C $chosen[1]
    end

    commandline -f repaint
end

bind {{chord}} __lore_pick
bind -M insert {{chord}} __lore_pick
