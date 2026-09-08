# PSReadLine owns the prompt line. Without it there is nothing to insert into,
# so the binding is skipped rather than failing at startup.
if (Get-Command Set-PSReadLineKeyHandler -ErrorAction SilentlyContinue) {
    Set-PSReadLineKeyHandler -Chord 'Ctrl+g' -ScriptBlock {
        # Newest first, in a file rather than in arguments. Windows hands a
        # child one string and lets it split its own arguments, so `cd C:\dir\`
        # escapes the quote meant to close it and swallows the command after it.
        # Blanks are dropped because Windows PowerShell removes an empty string
        # from an argument list outright.
        $recent = [IO.Path]::GetTempFileName()

        try {
            $commands = @(Get-History -Count 50 | ForEach-Object { $_.CommandLine } | Where-Object { $_ })
            [array]::Reverse($commands)

            # Explicit UTF-8 without a mark: the default here is the console
            # code page, which loses anything outside it.
            [IO.File]::WriteAllLines($recent, $commands, (New-Object Text.UTF8Encoding $false))

            $selected = & lore pick --shell powershell --history $recent
        } finally {
            Remove-Item $recent -Force -ErrorAction SilentlyContinue
        }

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
