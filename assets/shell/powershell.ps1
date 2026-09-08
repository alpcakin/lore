# PSReadLine owns the prompt line. Without it there is nothing to insert into,
# so the binding is skipped rather than failing at startup.
if (Get-Command Set-PSReadLineKeyHandler -ErrorAction SilentlyContinue) {
    Set-PSReadLineKeyHandler -Chord 'Ctrl+g' -ScriptBlock {
        # Windows PowerShell drops an empty string from a native command's
        # argument list, so passing one would leave --last without the value it
        # requires. A session with no history yet has nothing to offer anyway.
        $arguments = @('--shell', 'powershell')
        $last = (Get-History -Count 1).CommandLine
        if ($last) { $arguments += @('--last', $last) }

        $selected = & lore pick @arguments

        if ($LASTEXITCODE -ne 0) {
            # lore reports its own failures on the terminal. Redraw the prompt so
            # they are not left sitting on top of it.
            [Microsoft.PowerShell.PSConsoleReadLine]::InvokePrompt()
        } elseif ($selected) {
            [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
            [Microsoft.PowerShell.PSConsoleReadLine]::Insert(($selected -join ' '))
        }
    }
}
