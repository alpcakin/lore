__lore_pick() {
    local last selected
    last="$(fc -ln -1 2>/dev/null)"
    last="${last#"${last%%[![:space:]]*}"}"

    selected="$(lore pick --shell bash --last "$last")" || return
    [ -n "$selected" ] || return

    READLINE_LINE="$selected"
    READLINE_POINT=${#READLINE_LINE}
}

# `bind` only exists in an interactive shell, and `bind -x` only takes effect in
# emacs mode.
case $- in
    *i*) bind -x '"\C-g": __lore_pick' ;;
esac
