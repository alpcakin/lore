__lore_pick() {
    local selected recent status

    # Newest first, in a file rather than in arguments: Windows rebuilds a
    # child's argument list out of one string, so a command ending in a
    # backslash takes the next one with it. Every shell hands it over the same
    # way. A multi-line command still arrives as one entry per line, which fc
    # gives no way around.
    recent="$(mktemp)"
    fc -lnr -50 2>/dev/null | sed 's/^[[:space:]]*//' > "$recent"

    selected="$(lore pick --shell bash --history "$recent")"
    status=$?
    rm -f "$recent"

    [ $status -eq 0 ] || return
    [ -n "$selected" ] || return

    READLINE_LINE="$selected"
    READLINE_POINT=${#READLINE_LINE}
}

# `bind` only exists in an interactive shell, and `bind -x` only takes effect in
# emacs mode.
case $- in
    *i*) bind -x '"\C-g": __lore_pick' ;;
esac
