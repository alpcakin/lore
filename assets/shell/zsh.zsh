__lore_pick() {
    local last selected
    last="$(fc -ln -1 2>/dev/null)"
    last="${last#"${last%%[![:space:]]*}"}"

    selected="$(lore pick --shell zsh --last "$last")" || return
    if [[ -n "$selected" ]]; then
        BUFFER="$selected"
        CURSOR=${#BUFFER}
    fi

    zle reset-prompt
}

zle -N __lore_pick
bindkey '^G' __lore_pick
