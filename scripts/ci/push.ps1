# Pushes HEAD to a branch on GitHub from Azure Pipelines. First with the job's own credential
# (persistCredentials: true); if GitHub refuses it, with GH_TOKEN when the step maps that
# secret variable in, passed as a header so it never shows in a URL or the log.
#   scripts/ci/push.ps1 -Branch muse/perf-fix-16
param([Parameter(Mandatory)] [string]$Branch)
$ErrorActionPreference = 'Continue'
git push -q origin "HEAD:refs/heads/$Branch" 2>&1 | Out-Host
if ($LASTEXITCODE -eq 0) { Write-Host "pushed $Branch (job token)"; Write-Host "##vso[task.setvariable variable=pushedWith]job token"; exit 0 }
if (-not $env:GH_TOKEN) { Write-Host "##vso[task.logissue type=error]The push of $Branch was refused and GH_TOKEN is not mapped"; exit 1 }
$basic = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("x-access-token:$env:GH_TOKEN"))
Write-Host "##vso[task.setsecret]$basic"
$url = (git remote get-url origin)
git -c http.extraheader= -c "http.extraheader=AUTHORIZATION: basic $basic" push -q $url "HEAD:refs/heads/$Branch" 2>&1 | Out-Host
if ($LASTEXITCODE -eq 0) { Write-Host "pushed $Branch (GH_TOKEN)"; Write-Host "##vso[task.setvariable variable=pushedWith]GH_TOKEN"; exit 0 }
Write-Host "##vso[task.logissue type=error]The push of $Branch was refused with both credentials"; exit 1
