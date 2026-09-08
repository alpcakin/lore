# PSReadLine owns the prompt line. Without it there is nothing to insert into,
# so the binding is skipped rather than failing at startup.
if (Get-Command Set-PSReadLineKeyHandler -ErrorAction SilentlyContinue) {
    Set-PSReadLineKeyHandler -Chord 'Ctrl+g' -ScriptBlock {
        # An empty session history yields $null, which PowerShell drops from the
        # argument list entirely, leaving --last without the value it requires.
        $last = (Get-History -Count 1).CommandLine
        if (-not $last) { $last = '' }

        $selected = & lore pick --shell powershell --last "$last"

        if ($selected) {
            [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
            [Microsoft.PowerShell.PSConsoleReadLine]::Insert(($selected -join ' '))
        }
    }
}
