__lore_pick() {
    local selected recent status

    # Newest first, in a file rather than in arguments: Windows rebuilds a
    # child's argument list out of one string, so a command ending in a
    # backslash takes the next one with it. Every shell hands it over the same
    # way. A multi-line command still arrives as one entry per line, which fc
    # gives no way around.
    recent="$(mktemp)"
    fc -lnr -50 2>/dev/null | sed 's/^[[:space:]]*//' > "$recent"

    selected="$(lore pick --shell zsh --history "$recent")"
    status=$?
    rm -f "$recent"

    if [[ $status -eq 0 && -n "$selected" ]]; then
        BUFFER="$selected"
        CURSOR=${#BUFFER}
    fi

    zle reset-prompt
}

zle -N __lore_pick
bindkey '^G' __lore_pick
