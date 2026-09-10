# Renders docs/demo.gif with charmbracelet/vhs.
#
# The gif is generated rather than recorded so it can be redone whenever the
# interface changes, instead of slowly drifting away from what the tool does.
#
#   vhs docs/demo.tape
#
# Requires lore on PATH. The demo workflow builds it and runs this on Linux.

Output docs/demo.gif

Require lore

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 720
Set Padding 24
Set TypingSpeed 80ms

Hide
Type `eval "$(lore init bash)"` Enter
Type "clear" Enter
Show

Sleep 1s

# The picker opens under the prompt rather than taking over the screen.
Ctrl+G
Sleep 1500ms

# Searched by intent, not by remembering the flags.
Type "deleted"
Sleep 2s

# One placeholder, so the command lands in the prompt with the cursor in the
# gap and the shell's own completion available to finish it.
Enter
Sleep 1200ms
Type "parse_tags"
Sleep 2500ms
