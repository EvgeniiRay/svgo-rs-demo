# Pushes HEAD to a branch on GitHub from Azure Pipelines. First with the job's own credential
# (persistCredentials: true); if GitHub refuses it, with GH_TOKEN when the step maps that
# secret variable in, sent as a header in place of the job's one, so it never shows in a URL
# or the log.
#   scripts/ci/push.ps1 -Branch muse/perf-fix-16
param([Parameter(Mandatory)] [string]$Branch)
$ErrorActionPreference = 'Continue'
git push -q origin "HEAD:refs/heads/$Branch" 2>&1 | Out-Host
if ($LASTEXITCODE -eq 0) { Write-Host "pushed $Branch (job token)"; Write-Host "##vso[task.setvariable variable=pushedWith]job token"; exit 0 }
if (-not $env:GH_TOKEN -or $env:GH_TOKEN.StartsWith('$(')) { Write-Host "##vso[task.logissue type=error]The job token cannot push $Branch, and the secret variable GH_TOKEN is not set"; exit 1 }
$basic = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("x-access-token:$env:GH_TOKEN"))
Write-Host "##vso[task.setsecret]$basic"
$url = (git remote get-url origin)
# An empty value clears the job's header for this URL; the second value is the only one sent.
git -c "http.$url.extraheader=" -c "http.$url.extraheader=AUTHORIZATION: basic $basic" -c "http.extraheader=" push -q origin "HEAD:refs/heads/$Branch" 2>&1 | Out-Host
if ($LASTEXITCODE -eq 0) { Write-Host "pushed $Branch (GH_TOKEN)"; Write-Host "##vso[task.setvariable variable=pushedWith]GH_TOKEN"; exit 0 }
Write-Host "##vso[task.logissue type=error]The push of $Branch was refused with the job token and with GH_TOKEN"; exit 1
