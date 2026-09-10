__lore_pick() {
    local chosen recent status offset selected

    # Newest first, in a file rather than in arguments: Windows rebuilds a
    # child's argument list out of one string, so a command ending in a
    # backslash takes the next one with it. Every shell hands it over the same
    # way. A multi-line command still arrives as one entry per line, which fc
    # gives no way around.
    recent="$(mktemp)"
    fc -lnr -50 2>/dev/null | sed 's/^[[:space:]]*//' > "$recent"

    chosen="$(lore pick --shell bash --print-cursor --history "$recent")"
    status=$?
    rm -f "$recent"

    [ $status -eq 0 ] || return
    [ -n "$chosen" ] || return

    # The offset comes first on a line of its own, so the command is whatever
    # follows it and needs no parsing to recover.
    offset="${chosen%%$'\n'*}"
    selected="${chosen#*$'\n'}"

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
