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
    lore pick --shell bash --print-cursor --history "$recent" --output "$out"

    # The offset is first on a line of its own, so the command is the rest.
    offset="$(head -n 1 "$out")"
    selected="$(tail -n +2 "$out")"
    rm -f "$recent" "$out"

    [ -n "$selected" ] || return

    READLINE_LINE="$selected"
    # READLINE_POINT indexes bytes and the offset counts characters. The
    # interface is ASCII only, where the two are the same.
    READLINE_POINT=$offset
}

# `bind` only exists in an interactive shell, and `bind -x` only takes effect in
# emacs mode.
case $- in
    *i*) bind -x '"{{chord}}": __lore_pick' ;;
esac
