# Pushes HEAD to a branch on GitHub from Azure Pipelines. First with the job's own credential
# (persistCredentials: true); if GitHub refuses it, with GH_TOKEN when the step maps that
# secret variable in, sent as a header in place of the job's one, so it never shows in a URL
# or the log. A refused push is reported with git's own message.
#   scripts/ci/push.ps1 -Branch muse/perf-fix-16
param([Parameter(Mandatory)] [string]$Branch)
$ErrorActionPreference = 'Continue'
$env:GIT_TERMINAL_PROMPT = '0'
function Report($what, $lines) { $lines | Select-Object -Last 4 | ForEach-Object { Write-Host "##vso[task.logissue type=warning]$what`: $_" } }
$out = git push -q origin "HEAD:refs/heads/$Branch" 2>&1; $rc = $LASTEXITCODE; $out | Out-Host
if ($rc -eq 0) { Write-Host "pushed $Branch (job token)"; Write-Host "##vso[task.setvariable variable=pushedWith]job token"; exit 0 }
Report 'push with the job token' $out
Report 'credential config keys' (git config --name-only --get-regexp 'extraheader|credential')
if (-not $env:GH_TOKEN -or $env:GH_TOKEN.StartsWith('$(')) { Write-Host "##vso[task.logissue type=error]The job token cannot push $Branch, and the secret variable GH_TOKEN is not set"; exit 1 }
$basic = [Convert]::ToBase64String([Text.Encoding]::ASCII.GetBytes("x-access-token:$env:GH_TOKEN"))
Write-Host "##vso[task.setsecret]$basic"
$url = (git remote get-url origin)
# An empty value clears the headers set so far for this key; the value after it is the only one sent.
$out = git -c "http.$url.extraheader=" -c "http.$url.extraheader=AUTHORIZATION: basic $basic" -c "http.extraheader=" push -q origin "HEAD:refs/heads/$Branch" 2>&1; $rc = $LASTEXITCODE; $out | Out-Host
if ($rc -eq 0) { Write-Host "pushed $Branch (GH_TOKEN)"; Write-Host "##vso[task.setvariable variable=pushedWith]GH_TOKEN"; exit 0 }
Report "push with GH_TOKEN to $url" $out
Write-Host "##vso[task.logissue type=error]The push of $Branch was refused with the job token and with GH_TOKEN"; exit 1
