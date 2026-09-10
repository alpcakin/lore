function __lore_pick
    # Newest first, in a file rather than in arguments: Windows rebuilds a
    # child's argument list out of one string, so a command ending in a
    # backslash takes the next one with it. Every shell hands it over the same
    # way.
    set -l recent (mktemp)
    history --max 50 > $recent

    # The offset comes first on a line of its own, and fish splits command
    # substitution on newlines, so the first element is the offset and the rest
    # is the command.
    set -l chosen (lore pick --shell fish --print-cursor --history $recent)
    rm -f $recent

    if test (count $chosen) -ge 2
        commandline -r -- (string join \n $chosen[2..-1])
        commandline -C $chosen[1]
    end

    commandline -f repaint
end

bind {{chord}} __lore_pick
bind -M insert {{chord}} __lore_pick
