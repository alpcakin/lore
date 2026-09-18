__lore_pick() {
    local recent out offset selected

    # Newest first, in a file rather than in arguments: Windows rebuilds a
    # child's argument list out of one string, so a command ending in a
    # backslash takes the next one with it. Every shell hands it over the same
    # way. A multi-line command still arrives as one entry per line, which fc
    # gives no way around.
    recent="$(mktemp)"
    out="$(mktemp)"
    fc -lnr -50 2>/dev/null | sed 's/^[[:space:]]*//' > "$recent"

    # Deliberately not a command substitution. The picker draws a panel under
    # the prompt, which means asking the terminal where the cursor is, and that
    # question goes out on stdout. A captured stdout swallows it, no answer
    # comes back, and the panel never opens. The result comes back in a file so
    # stdout can stay attached to the terminal.
    #
    # stdin is redirected from the terminal by name because zle runs a widget's
    # commands with stdin on /dev/null. The picker would fall back to /dev/tty,
    # and on macOS the kernel refuses to poll that device, so the answer to
    # the cursor question would never be seen and the picker would wait for it
    # forever. $TTY is the device zsh itself is reading from.
    lore pick --shell zsh --print-cursor --history "$recent" --output "$out" < "$TTY"

    # The offset is first on a line of its own, so the command is the rest.
    offset="$(head -n 1 "$out")"
    selected="$(tail -n +2 "$out")"
    rm -f "$recent" "$out"

    if [[ -n "$selected" ]]; then
        BUFFER="$selected"
        CURSOR=$offset
    fi

    zle reset-prompt
}

zle -N __lore_pick
bindkey '{{chord}}' __lore_pick
