# Calls the GitHub REST API from Azure Pipelines. First with the job's own GitHub credential
# (the one Azure Pipelines persisted for this checkout, persistCredentials: true); if GitHub
# refuses it, with GH_TOKEN when the step maps that secret variable in. Neither is printed.
#   scripts/ci/gh-api.ps1 -Method POST -Path repos/OWNER/REPO/pulls -BodyFile body.json
param([string]$Method = 'GET', [Parameter(Mandatory)] [string]$Path, [string]$BodyFile)
$ErrorActionPreference = 'Stop'
$auths = @()
$header = git config --get-regexp 'http\..*\.extraheader' | Select-Object -First 1
if ($header) { $auths += ,@('job token', ($header -split 'AUTHORIZATION: ', 2)[1]) }
if ($env:GH_TOKEN) { $auths += ,@('GH_TOKEN', "Bearer $env:GH_TOKEN") }
if (-not $auths) { throw 'No GitHub credential: the checkout needs persistCredentials: true' }
foreach ($a in $auths) {
  $params = @{ Method = $Method; Uri = "https://api.github.com/$Path"; Headers = @{ Authorization = $a[1]; Accept = 'application/vnd.github+json'; 'User-Agent' = 'svgo-rs-azure' } }
  if ($BodyFile) { $params.Body = [IO.File]::ReadAllBytes((Resolve-Path $BodyFile)); $params.ContentType = 'application/json' }
  try { $r = Invoke-RestMethod @params; Write-Host "github api $Method $Path ok ($($a[0]))"; return $r }
  catch { Write-Host "github api $Method $Path refused with the $($a[0]): $($_.Exception.Response.StatusCode)" }
}
throw "GitHub refused $Method $Path"
