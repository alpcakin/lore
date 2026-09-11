# PSReadLine owns the prompt line. Without it there is nothing to insert into,
# so the binding is skipped rather than failing at startup.
if (Get-Command Set-PSReadLineKeyHandler -ErrorAction SilentlyContinue) {
    Set-PSReadLineKeyHandler -Chord '{{chord}}' -ScriptBlock {
        # Newest first, in a file rather than in arguments. Windows hands a
        # child one string and lets it split its own arguments, so `cd C:\dir\`
        # escapes the quote meant to close it and swallows the command after it.
        # Blanks are dropped because Windows PowerShell removes an empty string
        # from an argument list outright.
        $recent = [IO.Path]::GetTempFileName()
        $out = [IO.Path]::GetTempFileName()
        $chosen = @()

        try {
            $commands = @(Get-History -Count 50 | ForEach-Object { $_.CommandLine } | Where-Object { $_ })
            [array]::Reverse($commands)

            # Explicit UTF-8 without a mark: the default here is the console
            # code page, which loses anything outside it.
            [IO.File]::WriteAllLines($recent, $commands, (New-Object Text.UTF8Encoding $false))

            # The result is deliberately not assigned from the call. The picker
            # asks the terminal where the cursor is on stdout, and a captured
            # stdout swallows the question. It comes back in a file instead, and
            # is read as UTF-8 for the same reason it was written as UTF-8.
            & lore pick --shell powershell --print-cursor --history $recent --output $out

            if ($LASTEXITCODE -eq 0) {
                $chosen = @([IO.File]::ReadAllLines($out, (New-Object Text.UTF8Encoding $false)))
            }
        } finally {
            Remove-Item $recent, $out -Force -ErrorAction SilentlyContinue
        }

        # The picker draws over the prompt and erases it on the way out, leaving
        # the cursor on the row the prompt belongs on. PSReadLine renders the
        # edit buffer and nothing else, so the prompt is put back here; the row
        # is passed because a panel that scrolled the screen, or a message lore
        # printed, moved it away from where PSReadLine last saw it.
        [Microsoft.PowerShell.PSConsoleReadLine]::InvokePrompt($null, [Console]::CursorTop)

        # The offset is first on a line of its own, so the command is the rest.
        if ($chosen.Count -ge 2) {
            [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
            [Microsoft.PowerShell.PSConsoleReadLine]::Insert(($chosen[1..($chosen.Count - 1)] -join "`n"))
            [Microsoft.PowerShell.PSConsoleReadLine]::SetCursorPosition([int]$chosen[0])
        }
    }
}
