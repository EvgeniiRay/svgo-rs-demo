# nightly-sync.ps1: the Windows Task Scheduler variant of .github/workflows/nightly-sync.yml. It ports new upstream
# SVGO commits into svgo-rs with a headless `muse exec` run in a fresh worktree, pushes the sync branch and prints
# the pull request body.
#   SVGO_RS_REPO   this repository (default: the current folder)
#   SVGO_UPSTREAM  a clone of https://github.com/svg/svgo (default: ..\svgo)
#   MUSE           the muse command (default: muse on PATH)
#   MUSE_MODEL     the model (default: muse-wonder-1.0)
# Windows has no shell sandbox for headless runs yet, so this script passes --disable-sandbox; run it only on a
# machine and account you use for this job.
$ErrorActionPreference = 'Continue'
$repo = if ($env:SVGO_RS_REPO) { $env:SVGO_RS_REPO } else { (Get-Location).Path }
$up = if ($env:SVGO_UPSTREAM) { $env:SVGO_UPSTREAM } else { Join-Path (Split-Path $repo -Parent) 'svgo' }
$muse = if ($env:MUSE) { $env:MUSE } else { 'muse' }
$model = if ($env:MUSE_MODEL) { $env:MUSE_MODEL } else { 'muse-wonder-1.0' }
$date = Get-Date -Format yyyy-MM-dd
Set-Location $repo
Write-Host "$([char]27)[1msvgo-rs nightly sync$([char]27)[0m  $date"
git -C $up fetch -q origin main
$last = (Get-Content .svgo-sync -Raw).Trim()
$commits = @(git -C $up log --reverse --format='%h %s' "$last..origin/main" -- plugins lib)
if ($commits.Count -eq 0) { Write-Host "Up to date with upstream $last"; exit 0 }
Write-Host "$($commits.Count) upstream commits since $($last.Substring(0,7)):"; $commits | ForEach-Object { Write-Host "  svg/svgo@$_" }
$wt = Join-Path (Split-Path $repo -Parent) "svgo-rs-sync-$date"
git worktree add -q -b "sync/$date" $wt main
$task = (Get-Content scripts\nightly\nightly-task.md -Raw).Replace('{{DATE}}', $date).Replace('{{LAST}}', $last).
  Replace('{{UPSTREAM}}', $up).Replace('{{COMMITS}}', (($commits | ForEach-Object { "- svg/svgo@$_" }) -join "`n"))
New-Item -ItemType Directory -Force .sync | Out-Null
$taskFile = Join-Path $repo ".sync\task-$date.md"; Set-Content -Encoding utf8 $taskFile $task
Write-Host "muse exec --json --model $model --worktree existing --worktree-existing $wt --max-model-steps 300 --output-schema schemas\sync.schema.json`n"
& $muse exec --disable-sandbox --json --model $model --trust-workspace --approval-mode never --worktree existing --worktree-existing $wt `
  --max-model-steps 300 --output-schema schemas\sync.schema.json --prompt-file $taskFile 2>$null |
  Tee-Object -FilePath (Join-Path $repo ".sync\run-$date.jsonl") | node scripts\nightly\fmt-events.mjs
git -C $wt push -q origin "sync/$date"
Write-Host "`n$([char]27)[1mPull request:$([char]27)[0m sync/$date -> main"
Get-Content (Join-Path $wt ".sync\PR-$date.md") -ErrorAction SilentlyContinue
