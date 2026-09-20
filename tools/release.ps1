# Trigger the Release workflow on GitHub.
#
#   $env:GH_TOKEN = "<token with the workflow scope>"   # or a PAT with repo+workflow
#   ./tools/release.ps1 0.1.0
#   ./tools/release.ps1 0.2.0 -Prerelease -Draft -Notes "- first beta"
#
# The workflow itself tags v<version> and publishes the GitHub release, so the
# version must already be the one in Cargo.toml's [workspace.package].
param(
    [Parameter(Mandatory, Position = 0)][string]$Version,
    [switch]$Prerelease,
    [switch]$Draft,
    [string]$Notes = "",
    [string]$Repo = "raven-scratch/raven",
    [string]$Workflow = "release.yml",
    [string]$Ref = "main"
)

$ErrorActionPreference = "Stop"
$token = $env:GH_TOKEN
if (-not $token) { throw "set GH_TOKEN first (https://github.com/settings/tokens, workflow scope)" }

$body = @{
    ref    = $Ref
    inputs = @{
        version    = $Version
        prerelease = $Prerelease.IsPresent.ToString().ToLower()
        draft      = $Draft.IsPresent.ToString().ToLower()
        notes      = $Notes
    }
} | ConvertTo-Json -Depth 3

Invoke-RestMethod -Method Post `
    -Uri "https://api.github.com/repos/$Repo/actions/workflows/$Workflow/dispatches" `
    -Headers @{
        Authorization           = "Bearer $token"
        Accept                  = "application/vnd.github+json"
        "X-GitHub-Api-Version"  = "2022-11-28"
    } `
    -ContentType "application/json" -Body $body

"dispatched $Workflow for v$Version (prerelease=$($Prerelease.IsPresent), draft=$($Draft.IsPresent))"
"runs: https://github.com/$Repo/actions/workflows/$Workflow"
