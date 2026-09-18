# Packaging

What each file here is for, and what has to exist outside this repository
before it can be used.

## In this repository

| File                  | Used by                                                          |
|-----------------------|------------------------------------------------------------------|
| `install.sh`          | The macOS and Linux one liner in the README                      |
| `install.ps1`         | The Windows one liner in the README                              |
| `scoop/lore.json`     | Copied into the Scoop bucket repository                          |
| `homebrew/lore.rb`    | Copied into the Homebrew tap repository                          |
| `fill-checksums.sh`   | Writes a published release's checksums into the two above       |

Both installer scripts are served straight from `main` on raw.githubusercontent,
so a change to either takes effect the moment it is pushed. They resolve the
latest release through the GitHub API and need nothing else.

## What has to exist elsewhere

**`alpcakin/scoop-bucket`** - a repository holding `bucket/lore.json`. Users add
it with `scoop bucket add alpcakin https://github.com/alpcakin/scoop-bucket`.
The manifest has `checkver` and `autoupdate`, so a scheduled excavator run in
that repository keeps it current after the first release.

**`alpcakin/homebrew-tap`** - a repository holding `Formula/lore.rb`. The name
must start with `homebrew-` for `brew install alpcakin/tap/lore` to resolve.

**`CARGO_REGISTRY_TOKEN`** - a repository secret, so the release workflow can
publish to crates.io. Generate it at https://crates.io/settings/tokens with the
publish scope for `cmdlore`.

## Releasing

1. Update the version in `Cargo.toml` and the entry in `CHANGELOG.md`
2. Tag with `v<version>` and push the tag
3. The release workflow builds six targets and opens a draft release
4. Check the draft, then publish it
5. Run `packaging/fill-checksums.sh <version>`, which writes the checksums from
   `SHA256SUMS` into the Scoop manifest and the Homebrew formula, then push
   both to their own repositories. Without
   `CARGO_REGISTRY_TOKEN` the crates.io job skips with a warning rather than
   failing, so the archives are released either way
6. Submit to WinGet, which needs the release to be published first:

   ```
   wingetcreate update alpcakin.lore --version <version> --urls <zip url> --submit
   ```

Steps 5 and 6 are manual because they write to repositories this one has no
business holding a token for.
