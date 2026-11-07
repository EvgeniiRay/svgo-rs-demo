# nightly-sync.ps1: the scheduled job. Ports new upstream SVGO commits into svgo-rs with a headless muse exec run in
# a fresh worktree, pushes the sync branch, and prints the pull request body.
$ErrorActionPreference = 'Continue'
$repo = 'C:\demo\svgo-rs-public'; $up = 'C:\demo\svgo'; $date = Get-Date -Format yyyy-MM-dd
$muse = 'C:\demo\public-muse\muse-bin-1.4.14-R7309.1.exe'
$env:GIT_SSH_COMMAND = 'C:/Windows/System32/OpenSSH/ssh.exe'; $env:MUSE_NO_MODIFY_PATH = '1'
$env:PATH = "C:\demo\public-muse;$env:PATH"
$env:XDG_CONFIG_HOME = "C:\demo\profile-public\config"; $env:XDG_DATA_HOME = "C:\demo\profile-public\data"
Set-Location $repo
Write-Host "$([char]27)[1msvgo-rs nightly sync$([char]27)[0m  $date"
git -C $up fetch -q origin main
$last = (Get-Content .svgo-sync -Raw).Trim()
$commits = @(git -C $up log --reverse --format='%h %s' "$last..origin/main" -- plugins lib)
if ($commits.Count -eq 0) { Write-Host "Up to date with upstream $last"; exit 0 }
Write-Host "$($commits.Count) upstream commits since $($last.Substring(0,7)):"; $commits | ForEach-Object { Write-Host "  svg/svgo@$_" }
$wt = "C:\demo\svgo-rs-public-sync-$date"
git worktree add -q -b "sync/$date" $wt main
$task = (Get-Content scripts\nightly\nightly-task.md -Raw).Replace('{{DATE}}', $date).Replace('{{LAST}}', $last).
  Replace('{{COMMITS}}', (($commits | ForEach-Object { "- svg/svgo@$_" }) -join "`n"))
New-Item -ItemType Directory -Force .sync | Out-Null
$taskFile = "$repo\.sync\task-$date.md"; Set-Content -Encoding utf8 $taskFile $task
Write-Host "muse exec --json --model muse-wonder-1.0 --worktree existing --worktree-existing $wt --max-model-steps 300 --output-schema sync-result.schema.json`n"
& $muse exec --disable-sandbox --json --model muse-wonder-1.0 --trust-workspace --yolo --worktree existing --worktree-existing $wt `
  --max-model-steps 300 --output-schema scripts\nightly\sync-result.schema.json --prompt-file $taskFile 2>$null |
  Tee-Object -FilePath "$repo\.sync\run-$date.jsonl" | node scripts\nightly\fmt-events.mjs
git -C $wt push -q origin "sync/$date"
Write-Host "`n$([char]27)[1mPull request:$([char]27)[0m sync/$date -> main"
Get-Content "$wt\.sync\PR-$date.md" -ErrorAction SilentlyContinue
