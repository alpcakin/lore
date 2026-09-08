function __lore_pick
    set -l last (history --max 1)
    set -l selected (lore pick --shell fish --last "$last")

    if test -n "$selected"
        commandline -r -- $selected
    end

    commandline -f repaint
end

bind \cg __lore_pick
bind -M insert \cg __lore_pick
