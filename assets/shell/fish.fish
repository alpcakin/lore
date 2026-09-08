function __lore_pick
    # Newest first, in a file rather than in arguments: Windows rebuilds a
    # child's argument list out of one string, so a command ending in a
    # backslash takes the next one with it. Every shell hands it over the same
    # way.
    set -l recent (mktemp)
    history --max 50 > $recent

    set -l selected (lore pick --shell fish --history $recent)
    rm -f $recent

    if test -n "$selected"
        commandline -r -- $selected
    end

    commandline -f repaint
end

bind \cg __lore_pick
bind -M insert \cg __lore_pick
