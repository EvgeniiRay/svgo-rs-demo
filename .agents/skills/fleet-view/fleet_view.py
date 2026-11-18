#!/usr/bin/env python3
"""fleet-view: a colored tree of the fleet - coordinator, orchestrators, lanes (goal, branch, PR, status).

Reads live state: `herdr agent list` (locally and per saved Herdr machine), `git branch` over ssh (one call per
machine), GitHub GraphQL searches for PRs and owner decision cards, `herdr pane read` for lane messages and the
REVIEWERS · FIXERS section, the cron job files and the fleet loops' beats for the CRONS section, and the last scan state file
for its review gaps. It writes only when the owner
answers a decision in --watch mode: the answers file, the coordinator's decisions file and one line typed into the
coordinator's pane. Works with no config: it discovers Herdr machines and engagements, uses the signed-in gh user and
the current repo.
Optional config: ~/.fleet/view.json (see SKILL.md).
Usage: fleet_view.py [--plain] [--all] [--json] [--watch SECONDS] [--config PATH]
"""
import argparse, concurrent.futures as cf, datetime, hashlib, json, math, os, queue, re, shlex, shutil, subprocess
import platform, sys, tempfile, threading, time, unicodedata
try:
    import fcntl
except ImportError:                            # Windows has no fcntl: update_file then relies on its re-read retry
    fcntl = None

DEFAULT_CONFIG = os.path.expanduser("~/.fleet/view.json")
REFRESH_FILE = os.path.expanduser("~/.fleet/view.refresh")  # touch it to make a running --watch view refresh now
DECISIONS_FILE = os.path.expanduser("~/.fleet/decisions.json")  # the open owner decisions the coordinator keeps
ANSWERS_FILE = os.path.expanduser("~/.fleet/answers.jsonl")  # the owner's answers from the view, one JSON line each
RELAYS_FILE = os.path.expanduser("~/.fleet/relays.jsonl")  # the lines a relay typed into the coordinator's pane
CRONS_FILE = os.path.expanduser("~/.fleet/crons.json")  # the scheduled jobs coordinators publish (see SKILL.md)
# each fleet loop's <name>.beat, in the folder the loops read
LOOPS_DIR = os.path.expanduser(os.environ.get("FLEET_STATE_DIR") or "~/.fleet/state")
SCAN_FILE = os.path.expanduser("~/.fleet/state/scan.json")  # a scan state file, if one exists: its review gaps

# ── palette (truecolor) ────────────────────────────────────────────────────────────────────────────────────────────
def rgb(r, g, b): return f"\x1b[38;2;{r};{g};{b}m"
def bg(r, g, b): return f"\x1b[48;2;{r};{g};{b}m"
RESET, BOLD, DIM, ITAL = "\x1b[0m", "\x1b[1m", "\x1b[2m", "\x1b[3m"
C = {
    "frame": rgb(108, 118, 160), "coord": rgb(255, 196, 87), "orch": rgb(122, 162, 247), "machine": rgb(125, 207, 255),
    "lane": rgb(224, 222, 244), "goal": rgb(192, 202, 245), "branch": rgb(158, 206, 106), "muted": rgb(124, 132, 168),
    "open": rgb(115, 218, 202), "merged": rgb(187, 154, 247), "draft": rgb(120, 124, 153), "closed": rgb(247, 118, 142),
    "ok": rgb(158, 206, 106), "fail": rgb(247, 118, 142), "pend": rgb(224, 175, 104),
}
STATUS = {  # herdr agent_status, or the lane's health (see lane_health) -> (glyph, label, color)
    "working": ("●", "working", rgb(158, 206, 106)),
    "asking":  ("⁇", "asking",  rgb(255, 121, 198)),
    "queued":  ("↳", "queued",  rgb(255, 158, 100)),
    "blocked": ("◆", "blocked", rgb(247, 118, 142)),
    "idle":    ("○", "idle",    rgb(224, 175, 104)),
    "done":    ("✓", "done",    rgb(110, 118, 150)),
    "failed":  ("✗", "failed",  rgb(247, 118, 142)),   # a lane whose last run failed
    "unknown": ("?", "unknown", rgb(224, 175, 104)),
}
GRADIENT = [(255, 158, 100), (255, 196, 87), (158, 206, 106), (115, 218, 202), (122, 162, 247), (187, 154, 247)]


class Paint:
    def __init__(self, on, links=True): self.on, self.links = on, links and on
    def link(self, text, url):
        """OSC 8 hyperlink: Cmd/Ctrl-click opens it in terminals that support it; plain text elsewhere."""
        return f"\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\" if self.links and url else text
    def __call__(self, s, *codes): return "".join(codes) + s + RESET if self.on and codes else s
    def gradient(self, s):
        if not self.on: return s
        out, n = [], max(len(s) - 1, 1)
        for i, ch in enumerate(s):
            t = i / n * (len(GRADIENT) - 1); k = min(int(t), len(GRADIENT) - 2); f = t - k
            a, b = GRADIENT[k], GRADIENT[k + 1]
            out.append(rgb(*(round(a[j] + (b[j] - a[j]) * f) for j in range(3))) + ch)
        return BOLD + "".join(out) + RESET


ANSI = re.compile(r"\x1b\[[0-9;]*m|\x1b\]8;;[^\x1b]*\x1b\\")
def width(s):
    s = ANSI.sub("", s)
    return sum(2 if unicodedata.east_asian_width(c) in "WF" else 0 if unicodedata.combining(c) else 1 for c in s)
def clip(s, n):
    if n <= 0: return ""
    if width(s) <= n: return s
    out = ""
    for ch in s:
        if width(out + ch) > n - 1: break
        out += ch
    return out + "…"
def pad(s, n): return s + " " * max(n - width(s), 0)


def fit_line(s, n):
    """A painted line cut to `n` columns, its colors and links kept whole and closed, with "…" where it was cut."""
    if width(s) <= n: return s
    out, used, i = [], 0, 0
    while i < len(s):
        m = ANSI.match(s, i)
        if m:
            out.append(m.group()); i = m.end(); continue
        w = width(s[i])
        if used + w > n - 1: break
        out.append(s[i]); used += w; i += 1
    return "".join(out) + "…" + "\x1b]8;;\x1b\\" + RESET


# ── data ───────────────────────────────────────────────────────────────────────────────────────────────────────────
def run(cmd, timeout=60):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout)
        return p.stdout
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return ""


def agents_on(machine):
    cmd = ["herdr", "agent", "list"] if machine == "local" else ["herdr", "--machine", machine, "agent", "list"]
    out = run(cmd, timeout=60)
    try:
        return json.loads(out)["result"]["agents"]
    except (ValueError, KeyError, TypeError):
        return None  # machine unreachable


def branches_on(ssh, dirs, here=False):
    """({dir: branch}, stats, {dir: git root}, [run]) in one ssh round trip: each worktree dir's branch and git root,
    the host's load, memory and UTC offset (stats["tz"], seconds east of UTC). A Mac reads no load or memory.
    With no alias the trip runs on this host, so only `here` (this host's own read) makes it with no dirs: another
    machine with no alias and no lane gets no stats."""
    dirs = sorted({d for d in dirs if d})
    if not ssh and not dirs and not here: return {}, None, {}, []
    script = ('printf "__STATS__\\t%s\\t%s\\t%s\\t%s\\n" "$(cut -d" " -f1 /proc/loadavg 2>/dev/null)" "$(nproc 2>/dev/null)" '
              '"$(free -m 2>/dev/null | awk \'/Mem:/{print $3" "$2}\')" "$(date +%z 2>/dev/null)"; ')
    if dirs:
        script += "for d in " + " ".join(shlex.quote(d) for d in dirs) + \
            '; do b=$(git -C "$d" branch --show-current 2>/dev/null); ' \
            '[ -n "$b" ] || b=$(git -C "$d" rev-parse --short HEAD 2>/dev/null); ' \
            'printf "%s\\t%s\\t%s\\n" "$d" "${b:--}" "$(git -C "$d" rev-parse --show-toplevel 2>/dev/null)"; done; '
    cmd = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", ssh, script] if ssh else ["sh", "-c", script]
    out = run(cmd, timeout=60)
    br, stats, tops, runs = {}, None, {}, []
    for l in out.splitlines():
        parts = l.split("\t")
        if parts[0] == "__STATS__" and len(parts) in (4, 5):
            try:
                used, tot = (int(x) for x in parts[3].split())
                stats = {"load": float(parts[1]), "cores": int(parts[2]), "mem_used": used, "mem_total": tot}
            except ValueError:
                pass
            z = re.fullmatch(r"([+-])(\d\d)(\d\d)", parts[4]) if stats and len(parts) == 5 else None
            if z: stats["tz"] = (1 if z[1] == "+" else -1) * (int(z[2]) * 3600 + int(z[3]) * 60)
        elif parts[0] == "__RUN__" and len(parts) == 6 and parts[2] and (m := RUN_ID.fullmatch(parts[1])):
            runs.append({"run": parts[1], "pr": int(m.group(1)), "account": int(m.group(2)), "start": int(m.group(3)),
                         "anchor": parts[2], "head": parts[3] or None, "written": parts[4] == "1",
                         "repo": parts[5] if re.fullmatch(REPO_NAME, parts[5]) else None})
        elif len(parts) == 3:
            br[parts[0]] = parts[1]
            if parts[2]: tops[parts[0]] = parts[2]
    return br, stats, tops, runs


# GitHub reads in a --watch view (#7807): the view refreshes every few seconds, and when it read GitHub at each refresh
# the owner's account hit GitHub's secondary rate limit several times an hour. So in --watch every GitHub read goes
# through `github`: a read stands for GITHUB_EVERY seconds (`github_every` in the config) after its try, a failed read
# gives the last good data and waits as long, and touching REFRESH_FILE reads again at the next refresh
# (force_github). The footer tells the age of the data (github_age).
GITHUB_EVERY = 180                     # seconds between two reads of the same GitHub data in a --watch view
GITHUB = {"on": False, "every": GITHUB_EVERY, "reads": {}}   # reads: name -> {"key", "value", "at", "tried", "ok"}
# A touch of REFRESH_FILE reads again only what a steer or an assignment can change, at most once in FORCE_EVERY
# seconds (#8739): a coordinator that steers all the time touched it every 25 to 50 s, and each touch read every cache,
# A forced read is at most one extra set of calls a minute.
FORCE_EVERY = 60
FORCED_READS = ("prs", "pins", "reviews")    # the github() reads a touch reads again; login and repo wait
FORCED = {"at": None, "due": False}          # the last forced read, and a touch that waits for FORCE_EVERY


def github(name, key, read, now=None, covers=None):
    """read() for one GitHub read, through the --watch cache while GITHUB is on; else every call reads. The last read
    stands for GITHUB["every"] seconds after its try while its key covers `key` (equal, or `covers(old, new)`). A key
    that it does not cover reads at once, unless the last try failed. A read that gives None or raises ValueError
    failed: the last good value stands, with its time (`at`), and the next try waits as long."""
    if not GITHUB["on"]:
        try:
            return read()
        except ValueError:
            return None
    now = time.time() if now is None else now
    c = GITHUB["reads"].get(name)
    same = bool(c) and (covers(c["key"], key) if covers else c["key"] == key)
    if c and now - c["tried"] < GITHUB["every"] and (same or not c["ok"]):
        return c["value"] if same else None
    try:
        got = read()
    except ValueError:
        got = None
    if got is not None: GITHUB["reads"][name] = {"key": key, "value": got, "at": now, "tried": now, "ok": True}
    elif same: c.update(tried=now, ok=False)
    else: GITHUB["reads"][name] = {"key": key, "value": None, "at": 0, "tried": now, "ok": False}
    return GITHUB["reads"][name]["value"]


def force_github(now=None):
    """Touching REFRESH_FILE: the next refresh reads again the PR searches, the pins, the reviewers' PRs and claims,
    and the decision cards. The review clocks (they move in days), the login and the repository keep their
    periods. At most one forced read in FORCE_EVERY seconds: a touch inside that time stays due (FORCED["due"]), and
    the first refresh after it reads. True when this call forced the reads."""
    now = time.time() if now is None else now
    FORCED["due"] = True
    if FORCED["at"] is not None and now - FORCED["at"] < FORCE_EVERY:
        return False
    FORCED.update(at=now, due=False)
    for name in FORCED_READS:
        if name in GITHUB["reads"]: GITHUB["reads"][name]["tried"] = 0
    CARDS["at"] = 0
    return True


def every(own):
    """The period of a cache of its own (CARDS_EVERY, READY_EVERY): in --watch never under GITHUB["every"]."""
    return max(own, GITHUB["every"]) if GITHUB["on"] else own


def watch_github(cfg):
    """Turn on the --watch cache of GitHub reads, with the config's `github_every` seconds, else GITHUB_EVERY."""
    v = cfg.get("github_every")
    GITHUB.update(on=True, every=float(v) if number(v) and v > 0 else GITHUB_EVERY)


# GitHub lists a head's checks in the order they were created, at most 100 in one read. A head with a big CI matrix
# carries more (120 seen), and Buildkite posts its jobs last (#8930): read the oldest 100 and the newest 100, the whole
# list up to 200. Under 200 the two reads overlap, and a run read twice changes no check's state (see ci_state).
CONTEXT = ("nodes{... on CheckRun{name status conclusion startedAt completedAt checkSuite{workflowRun{workflow{name}}}} "
           "... on StatusContext{context state createdAt}}")
CHECKS = "oldest:contexts(first:100){" + CONTEXT + "} newest:contexts(last:100){" + CONTEXT + "}"
PR_FIELDS = ("number title mergedAt state isDraft headRefName url repository{name} reviewThreads(first:100){nodes{isResolved}} "
             "commits(last:1){nodes{commit{statusCheckRollup{state " + CHECKS + "}}}}")
# Open PRs carry checks and threads, so a page of 100 of them can time out (HTTP 504 with ~90 open PRs in one
# repository): read them one repository at a time, in pages of OPEN_PAGE. Each PR carries two check reads (#8930), so a
# page of 20 came close to GitHub's 10 s limit: a page holds 10, as pr-sweep's does (#8788).
OPEN_PAGE = 10
# A search reads at most OPEN_PAGES pages: 500 open PRs in a repository. On 2026-11-07 the owner had 110 open PRs in
# one repository, and a cap of 10 pages (100 PRs) stopped every scan with an error and froze the watch's PRs.
OPEN_PAGES = 50
# GitHub sometimes drops one heavy page and answers the same query a moment later: read a page up to SEARCH_TRIES
# times, SEARCH_WAIT seconds apart, before the search counts as unanswered.
SEARCH_TRIES, SEARCH_WAIT = 3, 2
OPEN_QUERY = ("query($q:String!,$after:String){search(query:$q,type:ISSUE,first:" + str(OPEN_PAGE) + ",after:$after)"
              "{pageInfo{hasNextPage endCursor} nodes{... on PullRequest{" + PR_FIELDS + "}}}}")
# A merged PR needs no checks or threads. Reading them for 100 merged PRs made GitHub time out (HTTP 504) for an author
# with many merges, and then no merged PR showed at all.
MERGED_QUERY = ("query($q:String!){search(query:$q,type:ISSUE,first:100){nodes{... on PullRequest{"
                "number title mergedAt state isDraft headRefName url repository{name}}}}}")
PASSED = ("SUCCESS", "NEUTRAL", "SKIPPED")


def ci_state(commit):
    """A head's CI, read check by check: SUCCESS, FAILURE or PENDING (GitHub's rollup state when it lists no checks). A
    check counts green when any of its runs for the head passed; only when none passed does its newest run count. When
    marking a PR ready and a push both start a run for one head, the cancelled duplicate fails its gate, and the
    rollup turns red although the newer run passed. The runs are the oldest and the newest 100 (CHECKS), in GitHub's
    order: a run in both reads is the same run twice, so it changes neither rule."""
    roll = (commit or {}).get("statusCheckRollup") or {}
    runs = {}
    for i, n in enumerate([n for part in ("oldest", "newest") for n in (roll.get(part) or {}).get("nodes") or []]):
        if not n: continue
        if "context" in n:                     # a commit status, as Buildkite posts it
            key, at = ("status", n["context"]), n.get("createdAt") or ""
            st = "pass" if n.get("state") == "SUCCESS" else "wait" if n.get("state") in ("PENDING", "EXPECTED") else "fail"
        else:                                  # a check run: its workflow and job name it
            wf = (((n.get("checkSuite") or {}).get("workflowRun") or {}).get("workflow") or {}).get("name")
            key, at = (wf, n.get("name")), n.get("startedAt") or n.get("completedAt") or "~"   # queued: the newest
            st = "wait" if n.get("status") != "COMPLETED" else "pass" if n.get("conclusion") in PASSED else "fail"
        runs.setdefault(key, []).append((at, i, st))
    if not runs: return roll.get("state")
    checks = ["pass" if any(r[2] == "pass" for r in rs) else max(rs)[2] for rs in runs.values()]
    return "FAILURE" if "fail" in checks else "PENDING" if "wait" in checks else "SUCCESS"


def pr_checks(n):
    """Give a PR from GitHub its `ci` (see ci_state) and `unresolved` (open review threads)."""
    c = n["commits"]["nodes"]
    n["ci"] = ci_state(c[0]["commit"]) if c else None
    n["unresolved"] = sum(1 for t in n["reviewThreads"]["nodes"] if not t["isResolved"])
    return n


def prs(cfg, strict=False):
    """{(repo, branch): pr} from one open-PR search per configured repository (one search over all repositories when
    `repos` is empty), read in pages of OPEN_PAGE, and one search over the PRs merged in the last 3 days (a merged PR
    gets no CI and no threads: MERGED_QUERY). A search reads at most OPEN_PAGES pages. When a later page fails, the
    pages already read still count. With `strict` a search that GitHub does not answer, or that has
    more than OPEN_PAGES pages, raises ValueError that says which: the scan must not read a PR that it did not get as no
    PR."""
    author, repos = cfg.get("github_author", "@me"), cfg.get("repos", [])
    scope = " ".join(f"repo:{r}" for r in repos)
    since = time.strftime("%Y-%m-%d", time.gmtime(time.time() - 3 * 86400))
    found = {}
    searches = [(f"is:pr is:open author:{author} repo:{r}", OPEN_QUERY) for r in repos] or \
               [(f"is:pr is:open author:{author}", OPEN_QUERY)]
    searches.append((f"is:pr is:merged merged:>={since} author:{author} {scope}", MERGED_QUERY))
    for q, query in searches:
        nodes, after, ok, capped = [], None, True, False
        for _ in range(OPEN_PAGES):  # at most OPEN_PAGES pages per search
            cmd = ["gh", "api", "graphql", "-f", f"query={query}", "-f", f"q={q}"]
            if after: cmd += ["-f", f"after={after}"]
            search = None
            for attempt in range(SEARCH_TRIES):
                if attempt: time.sleep(SEARCH_WAIT)
                try:
                    search = json.loads(run(cmd, timeout=60))["data"]["search"]
                    break
                except (ValueError, KeyError, TypeError):
                    search = None
            if search is None:
                ok = False
                break
            nodes += search["nodes"]
            page = search.get("pageInfo") or {}
            if not page.get("hasNextPage"): break
            after = page.get("endCursor")
        else:
            ok, capped = False, True   # GitHub has more pages than the cap reads
        if not ok:
            if strict and capped: raise ValueError(f"the PR search has more than {OPEN_PAGES} pages of {OPEN_PAGE}: {q}")
            if strict: raise ValueError(f"GitHub did not answer the PR search: {q}")
            if not nodes: continue
        for n in nodes:
            if not n: continue
            key = (n["repository"]["name"], n["headRefName"])
            if key in found and found[key]["state"] == "OPEN": continue
            found[key] = pr_checks(n) if query is OPEN_QUERY else dict(n, ci=None, unresolved=0)
    return found, list({(n["repository"]["name"], n["number"]): n for n in found.values()}.values())


def pinned(cfg):
    """{pin: pr} for config pins like "org/repo#123" (lanes whose worktree branch is not the PR branch). None when
    GitHub does not answer."""
    pins = sorted(set(cfg.get("pins", {}).values()))
    if not pins: return {}
    parts = []
    for i, pin in enumerate(pins):
        repo, num = pin.split("#")
        owner, name = repo.split("/")
        parts.append(f'p{i}:repository(owner:"{owner}",name:"{name}"){{pullRequest(number:{num}){{' + PR_FIELDS + '}}')
    out = run(["gh", "api", "graphql", "-f", "query={" + " ".join(parts) + "}"], timeout=60)
    try:
        data = json.loads(out)["data"]
    except (ValueError, KeyError, TypeError):
        return None
    if not isinstance(data, dict): return None
    res = {}
    for i, pin in enumerate(pins):
        n = (data.get(f"p{i}") or {}).get("pullRequest")
        if not n: continue
        res[pin] = pr_checks(n)
    return res


# A PR's review clock: a PR that no approval covers three days after it
# first went ready for review is stalled. GitHub's review decision APPROVED counts as an approval, and so does an
# approving review of the head (see approved). The view draws it red with its age, and a scan prints it at
# every scan, with the one step that unblocks it: the owning lane answers the review that covers its head, or a
# reviewer lane reviews a head that no review covers. The clocks come from their own search, read every READY_EVERY
# seconds: they move in days, and the fleet's GraphQL budget is shared. The search reads 100 PRs a page, up to
# READY_PAGES pages, with the cursor passed by hand: the owner had 126 open PRs on 2026-11-05, and the first 100 alone
# left the oldest 26 with no hold label, head or ready time.
READY_PAGES = 10
READY_QUERY = ("query($q:String!,$after:String){search(query:$q,type:ISSUE,first:100,after:$after){"
               "pageInfo{hasNextPage endCursor} nodes{... on PullRequest{number repository{name} "
               "createdAt isDraft headRefOid reviewDecision author{login} labels(first:20){nodes{name}} "
               "latestOpinionatedReviews(first:20){nodes{state commit{oid}}} "
               "latestReviews(first:20){nodes{state submittedAt author{__typename login} commit{oid}}} "
               "timelineItems(itemTypes:[READY_FOR_REVIEW_EVENT,CONVERT_TO_DRAFT_EVENT],first:1){nodes{__typename "
               "... on ReadyForReviewEvent{createdAt}}}}}}}")
READY_EVERY = 300                          # seconds between two reads of the review clocks
READY = {"key": None, "at": 0, "prs": {}}
STALL_DAYS = 3
# A PR with this label waits on purpose: the merge loops refuse it, and it never stalls, so neither the view
# nor does a scan ask anyone to push it. Research-only PRs carry it.
HOLD_LABEL = "do-not-merge"


def ready_of(n):
    """When a PR first went ready for review, in epoch seconds, or None when it never did: its first ready-for-review
    event, else its creation when it was opened ready (its first such event makes it a draft, or it has none and is no
    draft). A new head does not move the time, and a PR that goes back to draft keeps it."""
    first = next(iter((n.get("timelineItems") or {}).get("nodes") or []), None) or {}
    if first.get("__typename") == "ReadyForReviewEvent": return utc(first.get("createdAt"))
    if first.get("__typename") == "ConvertToDraftEvent" or not first and not n.get("isDraft"): return utc(n.get("createdAt"))
    return None


def approved_at_head(n):
    """True when an APPROVED review of the PR's headRefOid exists. A repo with no review rule gives no reviewDecision,
    so there this is the only sign of an approval."""
    head, nodes = n.get("headRefOid"), (n.get("latestOpinionatedReviews") or {}).get("nodes") or []
    return bool(head) and any(isinstance(r, dict) and r.get("state") == "APPROVED" and (r.get("commit") or {}).get("oid") == head
                              for r in nodes)


def approved(n):
    """True when GitHub's reviewDecision is APPROVED, or an approval covers the head (#8742). The decision follows the
    repo's own rule for stale approvals: an approval is kept across pushes, merges of main and restacks, so an approved
    PR whose head moved waits for CI or the queue, not for a review. A repo that dismisses stale approvals drops the
    decision at the next push."""
    return n.get("reviewDecision") == "APPROVED" or approved_at_head(n)


def reviewed_at_head(n):
    """True when a review by someone other than the PR's author covers its head: the latest review of a reviewer
    (latestReviews) on the headRefOid, in any state. Findings are posted as a COMMENTED review, which
    latestOpinionatedReviews leaves out, so only this read tells a head with findings from a head that no one reviewed."""
    head, me = n.get("headRefOid"), ((n.get("author") or {}).get("login") or "")
    nodes = (n.get("latestReviews") or {}).get("nodes") or []
    return bool(head) and any(isinstance(r, dict) and (r.get("commit") or {}).get("oid") == head
                              and ((r.get("author") or {}).get("login") or "") != me for r in nodes)


def reviewed_at_head_at(n):
    """When the newest review that covers the PR's head (see reviewed_at_head) was submitted, in epoch seconds, or None
    when no review with a time covers it. A scan counts how long that review waits for the lane's answer
   ."""
    head, me = n.get("headRefOid"), ((n.get("author") or {}).get("login") or "")
    ats = [utc(r.get("submittedAt")) for r in (n.get("latestReviews") or {}).get("nodes") or [] if isinstance(r, dict)
           and head and (r.get("commit") or {}).get("oid") == head and ((r.get("author") or {}).get("login") or "") != me]
    ats = [t for t in ats if t is not None]
    return max(ats) if ats else None


# Reviewers wait REVIEW_COOLDOWN seconds after their review of an older head before they review the new head of a
# PR with no P0, P1 or P2 label, on every host.
# The config's `review_bot` names the app whose reviews count; with no `review_bot`, any app's review counts:
# the config's `review_bot`, the app that the review seats post as. With no `review_bot`, any app's review counts.
REVIEW_COOLDOWN = 1800


def cooldown_until(n, bot=None):
    """When the review loop may serve the head of PR `n` (epoch seconds), or None when no wait applies: REVIEW_COOLDOWN
    after the newest review at another head by the app `bot` (any app when `bot` is None), on a PR with no P0, P1 or P2
    label (the loop serves a new head of a labeled PR at once). The loop serves the head at that second, not after it.
    A person's review, or another app's, starts no wait."""
    head = n.get("headRefOid")
    labels = {str(l.get("name") or "").upper() for l in (n.get("labels") or {}).get("nodes") or [] if isinstance(l, dict)}
    if not head or labels & {"P0", "P1", "P2"}: return None
    def ours(a):
        login = str(a.get("login") or "").lower()
        login = login[:-5] if login.endswith("[bot]") else login
        return a.get("__typename") == "Bot" and (bot is None or login == bot.lower())
    ats = [utc(r.get("submittedAt")) for r in (n.get("latestReviews") or {}).get("nodes") or [] if isinstance(r, dict)
           and ours(r.get("author") or {}) and (r.get("commit") or {}).get("oid") not in (None, head)]
    ats = [t for t in ats if t is not None]
    return max(ats) + REVIEW_COOLDOWN if ats else None


def ready_reviews(cfg):
    """{(repo, number): {"ready_at", "approved", "head", "held", "reviewed", "reviewed_at", "cooldown_until", "created"}}
    for the author's open PRs in the repos (see ready_of, approved, HOLD_LABEL, reviewed_at_head, reviewed_at_head_at
    and cooldown_until; created: when it was opened), from one search read
    page by page (READY_PAGES at most). None when GitHub does not answer a page: a part read would leave the PRs of the
    pages after it with no hold label."""
    author, repos = cfg.get("github_author", "@me"), cfg.get("repos", [])
    bot = cfg.get("review_bot") if isinstance(cfg.get("review_bot"), str) and cfg.get("review_bot") else None
    if not repos: return {}
    q = f"is:pr is:open author:{author} " + " ".join(f"repo:{r}" for r in repos)
    nodes, after = [], None
    for _ in range(READY_PAGES):
        cmd = ["gh", "api", "graphql", "-f", f"query={READY_QUERY}", "-f", f"q={q}"] + (["-f", f"after={after}"] if after else [])
        try:
            search = json.loads(run(cmd, timeout=60))["data"]["search"]
            if not isinstance(search["nodes"], list): return None
        except (ValueError, KeyError, TypeError):
            return None
        nodes += search["nodes"]
        page = search.get("pageInfo") or {}
        if not page.get("hasNextPage") or not page.get("endCursor"): break
        after = page["endCursor"]
    out = {}
    for n in nodes:
        if not isinstance(n, dict) or not isinstance(n.get("repository"), dict) or not n.get("number"): continue
        labels = [l.get("name") for l in (n.get("labels") or {}).get("nodes") or [] if isinstance(l, dict)]
        out[(n["repository"].get("name"), n["number"])] = {"ready_at": ready_of(n), "approved": approved(n),
                                                           "head": n.get("headRefOid"), "held": HOLD_LABEL in labels,
                                                           "reviewed": reviewed_at_head(n), "reviewed_at": reviewed_at_head_at(n),
                                                           "cooldown_until": cooldown_until(n, bot),
                                                           "created": utc(n.get("createdAt"))}
    return out


def ready_cached(cfg, now=None):
    """ready_reviews, read again only every READY_EVERY seconds (see every) or when the repos, the author or the review
    bot change.
    While GitHub does not answer, the last read stands, and the next try waits as long."""
    now = time.time() if now is None else now
    key = json.dumps([cfg.get("repos"), cfg.get("github_author"), cfg.get("review_bot")], sort_keys=True, default=str)
    if READY["key"] != key or now - READY["at"] >= every(READY_EVERY):
        got = ready_reviews(cfg)
        if got is not None or READY["key"] != key: READY.update(key=key, prs=got or {})
        READY["at"] = now
    return READY["prs"]


def stall_of(pr, now=None):
    """How long an open PR has waited for an approval since it first went ready, in seconds, when that is STALL_DAYS
    (3) days or more and it is not approved (see approved); else None. Its `ready_at`, `approved` and `held` come from
    ready_reviews: a PR its owner holds with HOLD_LABEL never stalls. A draft does not stall either, so its lane shows
    "mark it ready"; it keeps `ready_at`, and the clock counts on when it is ready again."""
    if not isinstance(pr, dict) or pr.get("state") != "OPEN" or pr.get("isDraft") or pr.get("approved") or \
            pr.get("held") or not number(pr.get("ready_at")): return None
    age = (time.time() if now is None else now) - pr["ready_at"]
    return age if age >= STALL_DAYS * 86400 else None


TOOL_LINE = re.compile(r"^(Bash|Read|Write|Edit|Update|MultiEdit|Grep|Glob|Search|Task|Agent|TodoWrite|WebFetch|WebSearch|"
                       r"Ran|Read|Searched|Edited|Listed|Explored|Waited|Wrote|Updated|Called|mcp__|Running|Sending message|"
                       r"Hooks running|Workflow|Worked for)\b|^[A-Za-z_]+\(|"
                       r"^Working \(|^Failed \(exit \d+\)")   # Codex's working line and its failed commands start with •


FOOTER = re.compile(r"^(Worked|Baked|Cooked|Brewed|Churned) for \d")   # turn-duration footers under a reply


def prose(t):
    """True for a line of English text, false for code, commands, tool output and spinner rows."""
    if t.startswith(("⎿", "└", "◯", "✻", "✶", "·", "$", "#", "{", "}", "|", ">")): return False
    sym = sum(1 for ch in t if not (ch.isalnum() or ch in " .,:;'’\"()-–—?!/#`*_"))
    return sym <= max(2, len(t) // 12)


def tidy(msg):
    msg = re.sub(r"\s·\s\d+m?\s?\d*s\b.*$", "", msg)      # spinner timer tail: " · 3m 5s ..."
    msg = re.sub(r"\s+", " ", msg).strip()
    return msg


def pane_text(machine, pane, lines=160):
    """The pane's recent text, from `herdr [--machine M] pane read`; empty when the pane or its machine is gone."""
    cmd = (["herdr"] + ([] if machine == "local" else ["--machine", machine]) +
           ["pane", "read", pane, "--source", "recent-unwrapped", "--lines", str(lines), "--format", "text"])
    return run(cmd, timeout=40)


def pulse_of(machine, pane):
    """The lane's latest message: a STATUS/BLOCKED line if present, else its last assistant paragraph.
    Reads the pane text, so it works for any harness Herdr hosts: Claude Code, Codex and Muse."""
    return pulse_in(pane_text(machine, pane))


def pulse_in(text):
    """pulse_of for pane text already read."""
    if not text: return ""
    lines = text.splitlines()
    msgs, cur = [], None
    for ln in lines:
        m = re.match(r"^\s*[●•⏺◆]\s+(.*)", ln)        # message bullet: Claude Code ● ⏺, Codex •, Muse ◆
        if m:
            if cur is not None: msgs.append(cur)
            body = m.group(1).strip()
            cur = None if TOOL_LINE.match(body) or not prose(body) else body
        elif cur is not None and re.match(r"^  \S", ln) and prose(ln.strip()) and not FOOTER.match(ln.strip()):
            if len(cur) < 400: cur += " " + ln.strip()
        elif cur is not None and ln.strip():
            msgs.append(cur); cur = None          # code, tool output or a spinner: the paragraph ends here
        elif cur is not None and not ln.strip():
            msgs.append(cur); cur = None
    if cur is not None: msgs.append(cur)
    msgs = [tidy(m) for m in msgs if len(tidy(m).split()) >= 3]
    for msg in reversed(msgs):
        if "BLOCKED(HUMAN)" in msg or msg.startswith("STATUS") or "STATUS:" in msg:
            return msg
    return msgs[-1] if msgs else ""


ASK_TAB = re.compile(r"☐\s+([^☐☒✔→]+?)(?:\s{2,}|\s*[☐☒✔→]|\s*$)")   # a Claude Code dialog's question tab: "☐ <header>"
ASK_FOOT = re.compile(r"Enter to select|Esc to cancel")   # the footer that an open dialog draws under its options
ASK_STRIP = re.compile(r"^ {0,2}(?:←\s*(?:[☐☒]|✔\s*Submit)|[☐☒])")   # a line of a dialog's tab strip: its first tab
ASK_ANY = "an open question or dialog"   # what the coordinator line says when the pane shows no dialog it can read


def dialog_in(text):
    """The open question of a Claude Code dialog (AskUserQuestion) in pane text, else "". The dialog draws its tabs
    ("☐ <header>", one for each question, then "✔ Submit"), a blank line, the question as "│ " lines, its options and a
    footer ("Enter to select · … · Esc to cancel"). Only the newest such block counts, and only while its footer is the
    pane's last line with text, with no message between: after an answer the pane goes on (an answer line, a spinner,
    a message or the prompt). With one question tab the result is "<header>: <question>"; with more, the text does
    not show which tab is open, so it is the question alone. A line of the tab strip starts, after at most two spaces,
    with its first tab, after "←" when there are more ("←  ✔ Submit  →" when no box shows). A line that starts with
    "←" and no tab is no strip. A question
    line starts with "│ ", an option with its number and an option's description with five spaces, so a box there
    is no strip. A todo list draws "☐ <task>" lines too, under "⎿" and five spaces in, so it is no strip either."""
    lines = (text or "").splitlines()
    i = next((k for k in range(len(lines) - 1, -1, -1) if ASK_STRIP.search(lines[k])), None)
    m = ASK_TAB.search(lines[i]) if i is not None else None
    if not m: return ""          # the newest strip decides: one with no open tab (Submit only) names none
    j = i + 1
    while j < len(lines) and not lines[j].strip(): j += 1
    q = []
    while j < len(lines) and re.match(r"^\s*│", lines[j]):
        q.append(re.sub(r"^\s*│\s?", "", lines[j]).strip()); j += 1
    rest = [l for l in lines[j:] if l.strip()]
    if not q or not rest or not ASK_FOOT.search(rest[-1]) or any(MESSAGE.match(l) for l in rest): return ""
    question = " ".join(x for x in q if x)
    return tidy(question if len(re.findall(r"[☐☒]", lines[i])) > 1 else m.group(1).strip() + ": " + question)


def coordinator_follow(pane, pulse):
    """One step of the --watch follower: the fields that it sets on the view's data, the coordinator's open dialog
    (coord_ask) and, with pulse, its newest message (coord_pulse, when it has one). With pulse off the pane is read
    only while Herdr reports the coordinator blocked."""
    if not pulse and (coordinator_state(pane) or ("",))[0] != "blocked": return {"coord_ask": ""}
    text = pane_text("local", pane)
    ask = coordinator_ask(text, coordinator_state(pane))   # the state read after the text: an answer meanwhile clears it
    msg = pulse_in(text) if pulse else ""
    return dict(coord_ask=ask, **({"coord_pulse": msg} if msg else {}))


def coordinator_ask(text, cs):
    """What the coordinator waits on, for its line: the open dialog in its pane text while Herdr reports it blocked
    (cs is coordinator_state), else ""."""
    return (dialog_in(text) or ASK_ANY) if cs and cs[0] == "blocked" else ""


# A harness at work, in the last lines of its pane. Claude Code, Codex and Muse print "esc to interrupt"; a narrow
# pane cuts it off Claude Code's footer, so its spinner line ("✽ Architecting…") counts too. Codex prints "Working (…)",
# and "Pursuing goal" in goal mode, where Herdr can read it as idle.
BUSY = re.compile(r"esc to interrupt|\bWorking \(|Pursuing goal|^\s*[·✢✳✶✻✽*]\s+\S+…")


def pane_status(text, herdr=None):
    """A configured pane's state: Herdr's `agent_status`, made working when the pane's last lines show its harness at
    work. Herdr's blocked (a question waits) stays. A pane Herdr lists no agent for is idle if it has text, else unknown."""
    if herdr == "blocked": return "blocked"
    tail = [l for l in (text or "").splitlines() if l.strip()][-12:]
    if herdr == "working" or any(BUSY.search(l) for l in tail): return "working"
    return herdr if herdr in ("idle", "done") else "idle" if text else "unknown"


# A lane's health beside Herdr's status, as the lane-health rules name it: `asking`
# when a question or a dialog waits for a person, `queued` when a Codex queue held the same first input for one tick.
# openai/codex draws both under its turn's status line, `• Working (2m 10s • esc to interrupt)` (its newest reasoning
# summary can stand in place of "Working"): the follow-up group `• Queued follow-up inputs` with one `↳` line per input,
# then the collapsed question `? N question[s]`. The badge counts only as a whole line near the input box, and the group
# only under the status line with no message after it, so prose that names them is neither.
ASK_BADGE = re.compile(r"^\s*\? \d+ questions?( · [0-9][0-9hms ]*)?( · [^·]+ to answer)?\s*$")
ASK_TITLE = re.compile(r"^[^A-Za-z]*Action Required")      # Codex: "[ ! ] Action Required | <goal> | <repo>"
QUEUE_HEAD = re.compile(r"^\s*(?:•\s+)?Queued follow-up inputs?\s*$")
TURN = re.compile(r" to interrupt\)\s*$|(^|[^A-Za-z])Working \(")
MESSAGE = re.compile(r"^\s*[●⏺•◆]\s")
QUEUED_MIN = 10        # one tick: the queued-input window is one tick
QUEUE_SEEN = {}        # (machine, pane) -> (first input, when a refresh first saw it); kept from refresh to refresh


def question_in(text):
    """The Codex question badge (`? 1 question · 4m 59s`) when it is a whole line among the pane's last 15 lines."""
    tail = [l for l in (text or "").splitlines() if l.strip()][-15:]
    return next((l.strip() for l in reversed(tail) if ASK_BADGE.match(l)), "")


def queue_in(text):
    """(inputs, first input) of a Codex queue under its turn, else None."""
    lines = (text or "").splitlines()
    head = max((i for i, l in enumerate(lines) if QUEUE_HEAD.match(l)), default=None)
    turn = max((i for i, l in enumerate(lines) if TURN.search(l)), default=None)
    if head is None or turn is None or turn > head or any(MESSAGE.match(l) for l in lines[head + 1:]): return None
    inputs = [re.sub(r"^\s*↳\s*", "", l).strip() for l in lines[head + 1:] if re.match(r"^\s*↳", l)]
    return (len(inputs), inputs[0]) if inputs else None


def lane_health(l, machine, now):
    """`asking` or `queued` for a lane (see above), else None. A Codex queue's clock starts at the first refresh that
    sees its first input and runs while the pane shows it, also under a question; a pane read with no queue drops it."""
    text, key = l.get("text") or "", (machine, l.get("pane"))
    q = queue_in(text) if l.get("agent") == "codex" else None
    if q:
        if QUEUE_SEEN.get(key, (None,))[0] != q[1]: QUEUE_SEEN[key] = (q[1], now)
    elif text:
        QUEUE_SEEN.pop(key, None)
    if l["status"] == "blocked" or ASK_TITLE.match(l.get("title") or "") or question_in(text): return "asking"
    if q and now - QUEUE_SEEN[key][1] >= QUEUED_MIN * 60: return "queued"
    return None


def lane_status(s):
    """The status the tree and the header draw: Herdr's blocked is a question or a dialog that waits, so it is asking."""
    return "asking" if s == "blocked" else s


def panes_of(o):
    """The panes an orchestrator entry names in `pane`: one pane id, or a list of them."""
    pn = o.get("pane")
    return [x for x in ([pn] if isinstance(pn, str) else pn or []) if x]


def pane_lane(engagement, cwd, pane):
    """A lane name for a pane whose agent has no name: its folder named `<engagement>-<lane>` (cmtdesk-w1 gives w1),
    else the pane id."""
    for part in reversed(cwd.split("/")):
        if part.startswith(engagement + "-") and len(part) > len(engagement) + 1:
            return part[len(engagement) + 1:]
    return pane


def current_repo_basename():
    """Basename of this checkout's git top level, else of the cwd. The view
    scopes lanes to this repo: a lane counts as related when the git root
    of its worktree has this basename, on any host."""
    top = run(["git", "rev-parse", "--show-toplevel"], timeout=10).strip()
    base = os.path.basename(top.rstrip("/")) if top else ""
    return base or os.path.basename(os.getcwd().rstrip("/"))


def lane_repo_matches(cwd, tops, here_name, hand=False):
    """True when a lane belongs to this checkout's repo: the git root of
    its worktree (as branches_on reported it) has this repo's basename.
    A config-named hand pane is related by definition. A lane whose root
    is unknown is unrelated, not shown."""
    if hand:
        return True
    root = tops.get(cwd) or tops.get(cwd.rstrip("/")) or ""
    return bool(root) and os.path.basename(root.rstrip("/")) == here_name


def coordinator_state(pane):
    """The coordinator's Herdr status and state counter. Herdr changes them when the coordinator starts or ends a
    turn, for every harness it tracks (Claude Code, Codex, Muse), so the view needs no harness hooks."""
    if not pane: return None
    for ag in agents_on("local") or []:
        if ag.get("pane_id") == pane:
            return ag.get("agent_status"), ag.get("state_change_seq")
    return None


CLOSED_STATES = {"resolved", "answered", "closed", "done"}


def closed_card(d):
    """True for a card that its writer closed and left in the file: its "status" is a closed state, or it has no status
    and carries a "resolved" time. An answered card moves to ~/.fleet/decisions-resolved.json, but on 2026-11-12 the
    coordinator and its tick agents had marked 21 answered cards "resolved" in place, and DECISIONS showed them all as
    open. A card with no status and no "resolved" time is open, or parked by the owner, and stays shown."""
    status = norm(d.get("status")).lower()
    return status in CLOSED_STATES if status else bool(norm(d.get("resolved")))


def read_decisions(cfg):
    """The decisions file as (cards, unreadable). The cards are the entries that DECISIONS shows: objects with a "q"
    that are not closed (see closed_card). A closed card is neither shown nor unreadable. Each of the others is
    {"name", "why"}, and so is a file that is not a JSON list: DECISIONS and the scan name them (#8946). The
    coordinator writes the file by hand, and on 2026-11-04 a card with "question" and "title" in place of "q" was
    dropped without a word, so the owner never saw it. A missing or empty file holds no decision, as its writers read
    it. Each record also names the file, which the scan points to."""
    path = os.path.expanduser(cfg.get("decisions_file", DECISIONS_FILE))
    shown = path.replace(os.path.expanduser("~"), "~", 1)
    try:
        with open(path) as f:
            text = f.read()
        items = json.loads(text) if text.strip() else []
    except FileNotFoundError:
        return [], []
    except (OSError, ValueError) as e:
        return [], [{"name": shown, "why": f"{shown} cannot be read: {norm(e)}", "file": shown}]
    if not isinstance(items, list):
        return [], [{"name": shown, "why": f"{shown} holds no JSON list", "file": shown}]
    cards, bad = [], []
    for i, d in enumerate(items, 1):
        if isinstance(d, dict) and d.get("q"):
            if not closed_card(d): cards.append(d)
            continue
        name = (isinstance(d, dict) and norm(d.get("id") or d.get("url"))) or f"entry {i}"
        if isinstance(d, dict):
            text = norm(d.get("question") or d.get("title"))
            why = f'{name} has no "q" (its keys: {", ".join(sorted(d)) or "none"})' + (f": {text}" if text else "")
        else:
            why = f"{name} is no object: {norm(json.dumps(d))[:60]}"
        bad.append({"name": name, "why": why, "file": shown})
    return cards, bad


def decisions_of(cfg):
    """The owner's open decisions, a JSON list the coordinator keeps: [{"q", "options": [...], "recommend", "lane", "url"}].
    read_decisions names the entries that this list leaves out."""
    return read_decisions(cfg)[0]


# ── answers ────────────────────────────────────────────────────────────────────────────────────────────────────────
def norm(s): return re.sub(r"\s+", " ", str(s or "")).strip()


def ask_id(d):
    """A decision's id: its question, url and lane. The view's answer links, the answers file and the removal from the
    decisions file all name a decision by it."""
    return hashlib.sha1(json.dumps([norm(d.get("q")), norm(d.get("url")), norm(d.get("lane"))]).encode()).hexdigest()[:12]


def read_text(path):
    """A file's text, "" when it is missing."""
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return ""


def update_file(path, change, tries=8):
    """Change a small file atomically and lose no other writer's change. Under an exclusive lock on <path>.lock, read
    the file, write change(text) to a temporary file beside it, and rename that over the file only when the file still
    holds the text that was read. A writer that takes no lock (the coordinator's own tools) and writes in between
    makes the change start again from its text. change() returns the new text, or None to leave the file as it is.
    Returns what was written, or None. The lock is <path>.lock for the path as given; a symlinked file keeps its link
    and its target changes. A write in the instant between the last read and the rename is lost: a script that takes
    the same lock is safe from that too."""
    name = os.path.abspath(os.path.expanduser(path))
    path = os.path.realpath(name)
    folder = os.path.dirname(path)
    os.makedirs(folder, exist_ok=True)
    os.makedirs(os.path.dirname(name), exist_ok=True)
    with open(name + ".lock", "a") as lock:
        if fcntl is not None: fcntl.flock(lock, fcntl.LOCK_EX)
        for _ in range(tries):
            old = read_text(path)
            new = change(old)
            if new is None: return None
            fd, tmp = tempfile.mkstemp(dir=folder, prefix="." + os.path.basename(path) + ".")
            try:
                with os.fdopen(fd, "w", encoding="utf-8") as f:
                    f.write(new)
                    f.flush()
                    os.fsync(f.fileno())
                if os.path.exists(path): os.chmod(tmp, os.stat(path).st_mode & 0o777)
                if read_text(path) != old: continue    # another writer changed it meanwhile: change its text instead
                os.replace(tmp, path)
                return new
            finally:
                if os.path.exists(tmp): os.unlink(tmp)
    raise OSError(f"{path} changed {tries} times while the view wrote it")


def close_decision(path, ask):
    """Remove an answered decision from the coordinator's decisions file (see update_file): the first open entry with
    its id goes (a closed copy of the same question, see closed_card, stays), and every other entry stays as the
    coordinator wrote it. False when no open entry has the id (the coordinator removed it first) or the file is no
    JSON list."""
    def drop(text):
        items = json.loads(text) if text.strip() else []
        hit = next((i for i, d in enumerate(items) if isinstance(d, dict) and ask_id(d) == ask["id"] and not closed_card(d)),
                   None) if isinstance(items, list) else None
        return None if hit is None else json.dumps(items[:hit] + items[hit + 1:], indent=2, ensure_ascii=False) + "\n"
    for _ in range(3):
        try:
            return update_file(path, drop) is not None
        except ValueError:                     # caught a writer in the middle of its write: read the file again
            time.sleep(0.1)
    return False


def answered(text):
    """The records in an answers file's text: one JSON object per line, others skipped."""
    out = []
    for line in text.splitlines():
        try:
            rec = json.loads(line)
        except ValueError:
            continue
        if isinstance(rec, dict) and rec.get("q"): out.append(rec)
    return out


def closed_ids(recs):
    """The ids that answer records close: each answer's own decision, and the lane rows it closed with it (`also`)."""
    return [i for r in recs for i in [ask_id(r)] + [a for a in (r.get("also") or []) if isinstance(a, str)]]


def answered_ids(cfg):
    """The ids (see closed_ids) of the decisions answered from the view, and the comment links they answered (see
    comment_link), from the answers file."""
    try:
        recs = answered(read_text(os.path.expanduser(cfg.get("answers_file", ANSWERS_FILE))))
    except (OSError, ValueError):              # unreadable, or a byte that is no UTF-8: no answers to hide
        return [], []
    return closed_ids(recs), sorted({norm(r.get("url")) for r in recs if comment_link(r.get("url"))})


class AlreadyAnswered(Exception):
    """Another view, or this one before a restart, answered the decision already: nothing was saved or sent."""


def answer_line(ask, answer):
    """The one line the coordinator's pane gets: DECISION ANSWER: <q> → <answer> (<url>), with no control characters."""
    line = f"DECISION ANSWER: {ask['q']} → {answer}" + (f" ({ask['url']})" if ask.get("url") else "")
    return clip(norm(re.sub(r"[\x00-\x1f\x7f-\x9f]", " ", line)), 2000)


def herdr_error(p):
    """What a failed herdr call says: its JSON error message, else its output, else its exit code."""
    out = (p.stdout or "") + (p.stderr or "")
    try:
        return json.loads(p.stdout)["error"]["message"]
    except (ValueError, KeyError, TypeError):
        return clip(norm(out), 160) or f"exit {p.returncode}"


def type_line(pane, line):
    """Type `line` into a Herdr pane and press Enter: herdr pane send-text, then send-keys enter. None when both
    worked, else what went wrong."""
    for cmd in (["herdr", "pane", "send-text", pane, line], ["herdr", "pane", "send-keys", pane, "enter"]):
        try:
            p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=20)
        except (OSError, subprocess.TimeoutExpired) as e:
            return f"{cmd[2]}: {e}"
        if p.returncode != 0: return f"{cmd[2]}: {herdr_error(p)}"
    return None


def send_answer(ask, choice, cfg, pane=None, opener=None, now=None):
    """Answer decision `ask` with its option number `choice`, in one step. An owner action (kind label or action)
    opens its link first. Then the answer goes into the answers file as {q, url, lane, answer, at}, with `also` (the
    lane rows it closed) when there are any, a decision from
    the decisions file leaves that file, and the coordinator's pane gets one typed line (see answer_line). Returns
    (answer, problem), where problem is None when every step worked. When opening or writing the answers file
    fails, it raises and nothing else happens; a decision the answers file closes already raises AlreadyAnswered,
    before any link opens."""
    now = time.time() if now is None else now
    if ask["id"] in answered_ids(cfg)[0]: raise AlreadyAnswered(ask["id"])
    if ask["kind"] == "choice":
        answer = ask["options"][choice]
    else:
        link = ask["links"][choice]
        try:
            if opener: opener(link)
        except OSError as e:
            raise OSError(f"cannot open {link}: {e}") from e
        answer = "opened" + (f" {link}" if link != ask["url"] else "")
    rec = json.dumps({"q": ask["q"], "url": ask["url"], "lane": ask["lane"], "answer": answer,
                      "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now)),
                      **({"also": list(ask["also"])} if ask.get("also") else {})}, ensure_ascii=False)
    def add(old):                              # one answer per decision, also from two views at once
        if ask["id"] in closed_ids(answered(old)): return None
        return old + ("\n" if old and not old.endswith("\n") else "") + rec + "\n"
    if update_file(cfg.get("answers_file", ANSWERS_FILE), add) is None:
        raise AlreadyAnswered(ask["id"])
    problems = []
    if ask["source"] == "file":
        try:
            close_decision(cfg.get("decisions_file", DECISIONS_FILE), ask)
        except OSError as e:
            problems.append(f"decisions file: {e}")
    err = type_line(pane, answer_line(ask, answer)) if pane else "no coordinator pane"
    if err: problems.append(err)
    return answer, "; ".join(problems) or None


class Courier:
    """Sends the answers picked in the view on one thread, one at a time and in the order they were picked, so two
    answers never mix their typed lines in the coordinator's pane. drain() waits for the ones not sent yet: q pressed
    right after Enter still sends."""
    def __init__(self, send):
        self.q, self.send = queue.Queue(), send
        threading.Thread(target=self.run, daemon=True).start()

    def run(self):
        while True:
            item = self.q.get()
            try:
                self.send(*item)
            finally:
                self.q.task_done()

    def put(self, *item): self.q.put(item)

    def drain(self, seconds):
        end = time.time() + seconds
        while self.q.unfinished_tasks and time.time() < end: time.sleep(0.05)
        return not self.q.unfinished_tasks


# ── decision cards ─────────────────────────────────────────────────────────────────────────────────────────────────
# An owner decision card on GitHub (the grill skill's template): a comment that starts "Decision: **<question>?**",
# lists "- Option A — **<name>**: ..." and ends "Recommendation: Option A — **<reason>**".
CARD_QUERY = ("query($q:String!){viewer{login} search(query:$q,type:ISSUE,first:50){nodes{"
              "... on Issue{number repository{name} comments(last:100){nodes{body createdAt url author{login}}}} "
              "... on PullRequest{number repository{name} comments(last:100){nodes{body createdAt url author{login}}}}}}}")
CARD_OPTION = re.compile(r"^\s*[-*]\s*Option ([A-Z])\b\s*[—–:-]*\s*(?:\*\*(.+?)\*\*|(.+?)(?::\s|$))", re.M)
CARD_PICK = re.compile(r"^\s*Recommendation:\s*(?:Option\s+)?([A-Z])\b", re.M)
# A later comment with a line that starts with one of these closes the card (bold or a list mark may come first).
CARD_CLOSED = re.compile(r"^\W{0,4}(?:Owner ratification|Owner decision|OWNER-DECISION)\b", re.M)
CARDS_EVERY = 300                          # seconds between two reads of the cards
CARDS = {"key": None, "at": 0, "cards": []}


def utc(ts):
    """Epoch seconds of a GitHub time (2026-10-29T17:42:43Z), or None."""
    try:
        return datetime.datetime.strptime(ts, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc).timestamp()
    except (TypeError, ValueError):
        return None


def card_of(body):
    """(question, options, recommended letter) of a decision card, or None when `body` is no card: a card starts with
    "Decision:" and asks a question in that first line."""
    text = (body or "").lstrip()
    if not text.startswith("Decision:"): return None
    first = text.splitlines()[0][len("Decision:"):]
    if "?" not in first: return None
    opts = [f"{m.group(1)} — {norm(m.group(2) or m.group(3))}" for m in CARD_OPTION.finditer(text)]
    pick = CARD_PICK.search(text)
    letter = pick.group(1) if pick and pick.group(1) in {o[0] for o in opts} else None   # "Recommendation: I would…"
    return norm(first.replace("**", "")), opts, letter


def decision_cards(cfg, now=None):
    """The owner's open decision cards on GitHub, as DECISIONS rows. A card is a comment that the owner's account
    (github_author) posted in the last `days` days (3) in one of the repos (decision_cards.repos, else repos), and that
    no later comment in its issue or PR closes with "Owner ratification", "Owner decision" or "OWNER-DECISION". One
    search finds their threads by the template's "Why a human" field. None when GitHub does not answer."""
    dc = cfg.get("decision_cards", {})
    if dc is False: return []
    dc = dc if isinstance(dc, dict) else {}
    repos, me = dc.get("repos") or cfg.get("repos") or [], cfg.get("github_author") or "@me"
    if not repos: return []
    now = time.time() if now is None else now
    since = now - float(dc.get("days", 3)) * 86400
    q = (f'"Why a human" in:comments commenter:{me} updated:>={time.strftime("%Y-%m-%d", time.gmtime(since))} ' +
         " ".join(f"repo:{r}" for r in repos))
    try:
        data = json.loads(run(["gh", "api", "graphql", "-f", f"query={CARD_QUERY}", "-f", f"q={q}"], timeout=60))["data"]
        nodes = data["search"]["nodes"]
    except (ValueError, KeyError, TypeError):
        return None
    if me == "@me": me = (data.get("viewer") or {}).get("login") or me   # gh did not name the user: GitHub does
    cards = []
    for t in nodes if isinstance(nodes, list) else []:
        t = t if isinstance(t, dict) else {}
        repo, num = (t.get("repository") or {}).get("name"), t.get("number")
        cs = [c for c in (t.get("comments") or {}).get("nodes") or [] if isinstance(c, dict)]
        for c in cs:
            card, at = card_of(c.get("body")), utc(c.get("createdAt"))
            if not card or at is None or at < since: continue
            if me != "@me" and (c.get("author") or {}).get("login") != me: continue
            if any(CARD_CLOSED.search(x.get("body") or "") for x in cs if (x.get("createdAt") or "") > c["createdAt"]):
                continue
            cards.append({"q": card[0], "options": card[1], "recommend": card[2], "url": c.get("url") or "",
                          "lane": f"{repo}#{num}" if repo and num else "", "source": "card", "at": at})
    return sorted(cards, key=lambda d: d["at"])


def cards_cached(cfg, now=None):
    """decision_cards, read again only every CARDS_EVERY seconds (see every) or when the repos or the author change.
    While GitHub does not answer, the last list stands, and the next try waits as long."""
    now = time.time() if now is None else now
    key = json.dumps([cfg.get("decision_cards"), cfg.get("repos"), cfg.get("github_author")], sort_keys=True, default=str)
    if CARDS["key"] != key or now - CARDS["at"] >= every(CARDS_EVERY):
        got = decision_cards(cfg, now)
        if got is not None or CARDS["key"] != key: CARDS.update(key=key, cards=got or [])
        CARDS["at"] = now
    return CARDS["cards"]


# ── the DECISIONS rows ─────────────────────────────────────────────────────────────────────────────────────────────
URL = re.compile(r"https?://[^\s)>\]]*[^\s)>\].,;:!?'\"]")      # a closing period or comma is the sentence's
LABEL_ASK = re.compile(r"\bhuman_verified\b", re.I)          # an ask for the owner's label: an action, not a choice
PLACEHOLDER = ("<question>", "<link>")                       # the rule's own text, echoed in a pane: no question


def page_of(url):
    """A GitHub link without its #fragment: the issue or PR that a comment link points into."""
    return str(url or "").split("#", 1)[0]


def comment_link(url):
    """True for a link to one GitHub comment or review: it names one card, never a whole PR."""
    return bool(re.search(r"#(?:issuecomment-|discussion_r|pullrequestreview-)\d+$", str(url or "")))


def pr_link(d, repos):
    """The PR a human_verified ask names: its url without the comment part, a PR or issue link in its question, or its
    #number in the first repo; else its url."""
    for u in [d["url"]] + URL.findall(d["q"]):
        m = re.match(r"https://github\.com/[^/\s]+/[^/\s]+/(?:pull|issues)/\d+", u or "")
        if m: return m.group()
    n = re.search(r"(?:^|[\s(])#(\d{1,7})\b", d["q"])
    repo = (repos or [""])[0]
    return f"https://github.com/{repo}/pull/{n.group(1)}" if n and "/" in repo else d["url"]


def short_link(url):
    """repo#123 for a GitHub PR or issue link, else the link."""
    m = re.match(r"https://github\.com/[^/]+/([^/]+)/(?:pull|issues)/(\d+)", url or "")
    return f"{m.group(1)}#{m.group(2)}" if m else url


def rec_index(options, rec):
    """The index of the recommended option: the option that is the recommendation or starts with it ("A" names
    "A save it" and "A — Phase at start"), else None."""
    r = norm(rec).lower()
    for i, o in enumerate(options if r else []):
        o = norm(o).lower()
        if o == r or o.startswith(r) and not o[len(r):len(r) + 1].isalnum(): return i
    return None


def blocked_ask(msg):
    """The question, url and recommended option of a BLOCKED(HUMAN) line: "<question> <link> (recommended:
    <option>)". None for a line with no BLOCKED(HUMAN), or one that holds the rule's own placeholders (<question>,
    <link>): that is no question."""
    msg = norm(msg)
    if "BLOCKED(HUMAN)" not in msg: return None
    q = msg.split("BLOCKED(HUMAN)", 1)[1].lstrip(": ")
    if any(ph in q for ph in PLACEHOLDER): return None
    rec = re.search(r"\s*\(recommended:\s*([^)]+)\)\s*$", q, re.I)
    q = q[:rec.start()] if rec else q
    link = ([None] + list(URL.finditer(q)))[-1]                       # the card's link comes last
    if link and not q[link.end():].strip(" .,;:!?"): q = q[:link.start()]   # nothing after the link but a stop
    return {"q": norm(q), "url": link.group() if link else "", "recommend": rec.group(1).strip() if rec else None}


def lane_asks(data):
    """Each lane's own BLOCKED(HUMAN) line as a DECISIONS row (see blocked_ask), with its id. A tree lane also gets
    `who`, <engagement>/<lane>: the name that a scan gives a question it escalates."""
    out = []
    for o, l in [(o, l) for o in data["orchestrators"] for l in o["lanes"]] + [(None, r) for r in data.get("reviewers") or []]:
        a = blocked_ask(l.get("pulse"))
        if not a: continue
        d = {"q": a["q"], "url": a["url"], "lane": norm(l.get("lane")), "source": "lane", "recommend": a["recommend"]}
        if o: d["who"] = f"{norm(o.get('engagement'))}/{d['lane']}"
        d["id"] = ask_id(d)
        out.append(d)
    return out


def relays_of(cfg, now=None):
    """The BLOCKED(HUMAN) lines that herdr-relay.sh typed into the coordinator's pane in the last 3 days, as [{"q",
    "url", "at"}] with `at` in epoch seconds. The relay adds one JSON line {"at", "to", "from", "text"} for each line
    that it sent to ${FLEET_RELAYS_FILE:-~/.fleet/relays.jsonl}. The view reads `relays_file` if set, else the relay's
    own FLEET_RELAYS_FILE, so both name one file. A record with no valid time, a time after `now`, or a text with no
    BLOCKED(HUMAN) is skipped."""
    now = time.time() if now is None else now
    try:
        text = read_text(os.path.expanduser(cfg.get("relays_file") or os.environ.get("FLEET_RELAYS_FILE") or RELAYS_FILE))
    except (OSError, ValueError):              # unreadable, or a byte that is no UTF-8: no row is marked
        return []
    out = []
    for line in text.splitlines():
        try:
            r = json.loads(line)
        except ValueError:
            continue
        at = utc(r.get("at")) if isinstance(r, dict) else None
        a = blocked_ask(r["text"]) if at and isinstance(r.get("text"), str) else None
        if a and now - 3 * 86400 <= at <= now + 300:
            out.append({"q": a["q"], "url": a["url"], "at": at})
    return out


def relayed_at(d, relays):
    """When a relayed line last asked the question of row `d`, or None: the same question, or the same comment link (a
    PR or issue link names no one question)."""
    return max((r["at"] for r in relays if r["q"] == d["q"] or comment_link(d["url"]) and r["url"] == d["url"]), default=None)


def shadows(entry, lanes):
    """The lane rows that a listed decision stands for: the same lane (by its name, or as <engagement>/<lane>), or a
    link into the same issue or PR. They do not show while it is listed, and answering it closes them too."""
    lane, page = norm(entry.get("lane")), page_of(norm(entry.get("url")))
    return [a for a in lanes if lane and lane in (a["lane"], a.get("who")) or page and page_of(a["url"]) == page]


def open_asks(data, gone=()):
    """The DECISIONS rows, in order: the coordinator's decisions file, the decision cards from GitHub, then each lane's
    own BLOCKED(HUMAN) line that neither stands for (see shadows). A row answered from the view is closed and left
    out: its id is in the answers file or in `gone` (sent since the last refresh), or it links the GitHub comment of
    an answered card. Each row gets its id (ask_id), its source (file, card or lane), when a relayed line asked it
    (relayed_at) and its kind: a choice, an action (each option opens a link) or a label (a human_verified ask: Enter
    opens its PR)."""
    entries = [dict(d, source=d.get("source") or "file") for d in data.get("decisions") or []]
    lanes = lane_asks(data)
    listed = {a["id"] for e in entries for a in shadows(e, lanes)}
    done, links, out = set(data.get("answered") or ()) | set(gone), set(data.get("answered_links") or ()), []
    for d in entries + [a for a in lanes if a["id"] not in listed]:
        opts = d.get("options") if isinstance(d.get("options"), list) else []
        d.update(q=norm(d.get("q")), url=norm(d.get("url")), lane=norm(d.get("lane")), options=[norm(o) for o in opts if norm(o)])
        d["id"] = ask_id(d)
        if d["id"] in done or d["url"] in links or not d["q"]: continue
        d["relayed"] = relayed_at(d, data.get("relays") or [])
        if LABEL_ASK.search(" ".join([d["q"]] + d["options"])):
            link = pr_link(d, data.get("repos"))
            d.update(kind="label", options=[f"open {short_link(link)}" if link else "open"], links=[link], recommend=None)
        elif d["options"] and all(re.match(r"open(?:\s|$)", o, re.I) for o in d["options"]):
            found = [URL.search(o) for o in d["options"]]                    # "Open <link>" opens that link
            d.update(kind="action", links=[m.group() if m else d["url"] for m in found])
        else:
            d.update(kind="choice", links=None, options=d["options"] or ([norm(d["recommend"])] if d.get("recommend") else []))
        out.append(d)
    return out


def default_option(ask):
    """The option a newly picked decision starts on: its action, or its recommended option (None when it has none)."""
    if ask["kind"] != "choice": return 0 if ask["options"] else None
    return rec_index(ask["options"], ask.get("recommend"))


# ── crons ──────────────────────────────────────────────────────────────────────────────────────────────────────────
CRON_RANGES = ((0, 59), (0, 23), (1, 31), (1, 12), (0, 6))   # minute, hour, day of month, month, day of week
CRON_PART = re.compile(r"\*(?:/(\d{1,4}))?|(\d{1,4})-(\d{1,4})(?:/(\d{1,4}))?|(\d{1,4})")


# A bad value in a job file never stops the view: numbers are finite and bounded, times are plausible epoch seconds.
def number(x): return isinstance(x, (int, float)) and not isinstance(x, bool) and -1e15 < x < 1e15
def epoch(x): return float(x) if number(x) and 0 < x < 1e11 else None
def pid_of(x): return x if isinstance(x, int) and not isinstance(x, bool) and 0 < x < 2 ** 31 else None


def cron_field(text, lo, hi):
    """The sorted values one cron field allows: * or */n, n, a-b or a-b/n, or a list of them; in the day of the week 7
    is Sunday too. None when the field is not valid. It is the grammar Claude Code's scheduler reads."""
    week, out = (lo, hi) == (0, 6), set()
    for part in text.split(","):
        m = CRON_PART.fullmatch(part)
        if not m: return None
        star, a, b, step, one = m.groups()
        if one is not None: a = b = one
        elif a is None: a, b, step = lo, hi, star
        a, b, step = int(a), int(b), int(step or 1)
        if step < 1 or a > b or a < lo or b > (7 if week else hi): return None
        out.update(v % 7 if week else v for v in range(a, b + 1, step))
    return sorted(out)


def cron_parse(expr):
    """The value lists of a 5-field cron expression (minute, hour, day of month, month, day of week); None when it is
    not one."""
    parts = expr.split() if isinstance(expr, str) else []
    if len(parts) != 5: return None
    fields = [cron_field(t, lo, hi) for t, (lo, hi) in zip(parts, CRON_RANGES)]
    return None if None in fields else fields


def cron_day(c, d):
    """True when parsed cron `c` runs on date `d`. When both day fields are set, either one may match (cron's rule)."""
    if d.month not in c[3]: return False
    dom, dow = d.day in c[2], d.isoweekday() % 7 in c[4]          # cron counts Sunday as 0
    return dom or dow if len(c[2]) < 31 and len(c[4]) < 7 else dom and dow


def cron_near(c, t, ahead=True):
    """The first minute after `t` (ahead) or the last minute at or before it that parsed cron `c` matches, in local
    time, as epoch seconds; None when no minute matches within a year ahead, or four years back (29 February)."""
    lt = time.localtime(t)
    day, step = datetime.date(lt.tm_year, lt.tm_mon, lt.tm_mday), 1 if ahead else -1
    for i in range(367 if ahead else 1462):
        d = day + datetime.timedelta(days=i * step)
        if not cron_day(c, d): continue
        for h in c[1][::step]:
            if i == 0 and (h - lt.tm_hour) * step < 0: continue
            for m in c[0][::step]:
                if i == 0 and h == lt.tm_hour and (m <= lt.tm_min if ahead else m > lt.tm_min): continue
                ts = time.mktime((d.year, d.month, d.day, h, m, 0, 0, 0, -1))   # mktime settles daylight saving time
                if (ts > t) if ahead else (ts <= t): return ts
    return None


def cron_window(c, now, last=None):
    """(last fire, next fire) of parsed cron `c` around `now`; None when it never fires. Claude Code runs a recurring
    job a fixed time after its slot (up to half the period, 30 min at most), so a known last fire moves both by its
    own delay when the delay fits those limits: the bar then fills up to the real fire."""
    off, slot = 0, cron_near(c, last, ahead=False) if last else None
    if slot is not None:
        after = cron_near(c, slot)
        if after is not None and last - slot < min((after - slot) / 2, 1800):
            off = last - slot
    prev, nxt = cron_near(c, now - off, ahead=False), cron_near(c, now - off)
    return None if prev is None or nxt is None else (prev + off, nxt + off)


CRON_MEMO = {}  # (schedule, last fire) -> (window, valid from, valid until)


def cron_window_memo(expr, c, now, last):
    """cron_window, kept while it holds (a minute when there is no fire): the bars redraw 8 times a second, and a
    sparse schedule walks years of days."""
    hit = CRON_MEMO.get((expr, last))
    if hit and hit[1] <= now < hit[2]: return hit[0]
    w = cron_window(c, now, last)
    if len(CRON_MEMO) > 512: CRON_MEMO.clear()
    CRON_MEMO[(expr, last)] = (w,) + (w or (now, now + 60))
    return w


def cron_state(j, now):
    """(state, bar fraction, last fire, next fire) of a CRONS row at `now`. run: the bar fills from the last fire to
    the next and starts again from 0 when the job fires. firing: a heartbeat loop is due, and its bar waits at 100%
    until the loop touches its file. stale: a heartbeat two rounds old. dead: a heartbeat loop that ended or whose pid
    is gone. late: a one-shot task past its time. stopped, bad (no valid schedule), never (no fire within a year) and
    unseen (no fire seen yet) draw no bar either."""
    if j.get("stopped"): return "stopped", 0.0, None, None
    if j.get("dead") or j.get("ended"): return "dead", 0.0, j.get("last"), None
    every = j.get("every")
    if every:
        last = j.get("last")
        if last is None: return "unseen", 0.0, None, None
        if j.get("beat"):                    # the heartbeat shows each real fire, so the bar waits for it
            age = now - last
            if age >= 2 * every: return "stale", 1.0, last, None
            if age >= every: return "firing", 1.0, last, None
            return "run", max(age, 0) / every, last, last + every
        prev = last + max((now - last) // every, 0) * every       # no heartbeat: it fires on time from its last fire
        return "run", max(now - prev, 0) / every, prev, prev + every
    c = cron_parse(j.get("schedule"))
    if c is None: return "bad", 0.0, None, None
    if j.get("once"):
        start = j.get("created") or now
        fire = cron_near(c, start)
        if fire is None: return "never", 0.0, None, None
        if now >= fire: return "late", 1.0, start, fire
        return "run", max(now - start, 0) / (fire - start), start, fire
    w = cron_window_memo(j["schedule"], c, now, j.get("last"))
    if w is None: return "never", 0.0, None, None
    return "run", (now - w[0]) / (w[1] - w[0]), w[0], w[1]


BEAT_LINE = re.compile(r"(\d{1,11}) (\d{1,10}) (\d{1,7}) (\S+)(?: ended (.*))?")
LOOP_NAME = re.compile(r"[a-z0-9][a-z0-9-]{0,39}")


def beat_of(path):
    """The beat a fleet loop writes (the fleet skill's loops/README.md), "<epoch> <pid> <every_s> <script> [ended
    <reason>]", as (time, pid, every, reason it ended or None, script); None when the file holds no such line."""
    try:
        with open(path, errors="replace") as f:
            m = BEAT_LINE.fullmatch(f.readline(1024).rstrip("\n"))
    except (OSError, ValueError):
        return None
    if not m or epoch(int(m[1])) is None or pid_of(int(m[2])) is None or int(m[3]) < 1: return None
    why = None if m[5] is None else clip("".join(c if c.isprintable() else " " for c in m[5]).strip(), 120) or "?"
    return float(m[1]), int(m[2]), float(m[3]), why, m[4]


def cron_registry(cfg):
    """The jobs coordinators publish in ~/.fleet/crons.json (`crons_file` moves it): a JSON list of {name, schedule or
    every_s, source, machine, last_fire_ts, pid, heartbeat}. An entry needs a name and a schedule, or an every_s of at
    least one second; others are dropped."""
    try:
        with open(os.path.expanduser(cfg.get("crons_file", CRONS_FILE))) as f:
            items = json.load(f)
    except (OSError, ValueError):
        return []
    def ok(j):
        return (isinstance(j, dict) and isinstance(j.get("name"), str) and j["name"].strip() != "" and
                (isinstance(j.get("schedule"), str) or number(j.get("every_s")) and j["every_s"] >= 1))
    return [j for j in items if ok(j)] if isinstance(items, list) else []


def pid_alive(pid):
    """True when process `pid` runs on this machine, False when it is gone, None when `pid` is no process id."""
    if pid_of(pid) is None: return None
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True                          # it runs as another user
    except (OSError, OverflowError):
        return None
    return True


def runs_script(pid, script):
    """False when process `pid` runs a program other than the loop script `script` (a reused pid), as the supervisor's
    runs_script reads it; True when it runs the script, None when ps can't tell."""
    try:
        p = subprocess.run(["ps", "-ww", "-o", "command=", "-p", str(pid)], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None
    cmd = f" {p.stdout.strip()} "
    if p.returncode != 0 or cmd.isspace(): return None   # it ended since pid_alive looked, or ps failed
    return f"/{script} " in cmd or f" {script} " in cmd


def loop_dead(line, alive):
    """Why a loop whose beat is `line` (beat_of) and whose pid is `alive` (pid_alive) is dead, or None."""
    _, pid, _, ended, script = line
    if ended or alive is None: return None
    if alive is False: return f"pid {pid} gone"
    return f"pid {pid} reused" if runs_script(pid, script) is False else None


def is_here(machine, cfg):
    """Whether a machine name means this host: `local`, this host's name, a name marked "(local)" (the coordinator's
    default) or the coordinator's name without that mark."""
    name = lambda m: str(m or "").replace("(local)", "").strip()
    coord, m = str((cfg.get("coordinator") or {}).get("machine") or ""), name(machine)
    return m in ("", "local", platform.node().split(".")[0]) or "(local)" in str(machine or "") or \
        "(local)" in coord and m == name(coord)


def cron_ssh(machine, cfg):
    """The ssh alias that reads a job on another machine, or None for a job on this one. Another machine is one with
    an `ssh` alias in `orchestrators`, which holds the Herdr machines where lanes run. A name is_here calls this host
    is this machine, and so is any other name, such as a nickname."""
    if is_here(machine, cfg): return None
    name = lambda m: str(m or "").replace("(local)", "").strip()
    m = name(machine)
    return next((o["ssh"] for o in cfg.get("orchestrators") or [] if name(o.get("machine")) == m and o.get("ssh")), None)


def cron_probe(ssh, beats, pids):
    """{"mtime": {heartbeat: epoch}, "alive": [pid]} on another machine, in one ssh round trip; None when it doesn't
    answer."""
    sh = []
    for b in beats:
        q = '"$HOME"/' + shlex.quote(b[2:]) if b.startswith("~/") else shlex.quote(b)
        sh.append(f'm=$(stat -c %Y {q} 2>/dev/null || stat -f %m {q} 2>/dev/null) && printf "hb\\t%s\\t%s\\n" {shlex.quote(b)} "$m"')
    sh += [f'ps -p {p} >/dev/null 2>&1 && printf "pid\\t{p}\\n"' for p in pids]
    out = run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", ssh, "; ".join(sh + ["echo __END__"])], timeout=30)
    if "__END__" not in out: return None
    marks = {"mtime": {}, "alive": []}
    for line in out.splitlines():
        f = line.split("\t")
        if f[0] == "hb" and len(f) == 3:
            try: marks["mtime"][f[1]] = float(f[2])
            except ValueError: pass
        elif f[0] == "pid" and len(f) == 2 and f[1].isdigit():
            marks["alive"].append(int(f[1]))
    return marks


def cron_marks(cfg):
    """{machine: cron_probe result} for the registry's jobs on other machines (see cron_ssh) that name a heartbeat or
    a pid: one ssh round trip per machine."""
    want = {}
    for j in cron_registry(cfg):
        m = str(j.get("machine") or "")
        ssh = cron_ssh(m, cfg)
        if ssh is None: continue
        _, beats, pids = want.setdefault(m, (ssh, set(), set()))
        if isinstance(j.get("heartbeat"), str): beats.add(j["heartbeat"])
        if pid_of(j.get("pid")): pids.add(j["pid"])
    with cf.ThreadPoolExecutor(max_workers=8) as ex:     # a machine that doesn't answer delays no other
        jobs = {m: ex.submit(cron_probe, ssh, sorted(b), sorted(p)) for m, (ssh, b, p) in want.items() if b or p}
    return {m: f.result() for m, f in jobs.items()}


def durable_folders():
    """[(folder, label)] where Claude Code keeps durable cron tasks: the repository the view runs in, and home."""
    home = os.path.expanduser("~")
    try:
        here = d = os.getcwd()
    except OSError:                          # the folder was removed under the view
        return [(home, "home")]
    while not os.path.exists(os.path.join(d, ".git")) and os.path.dirname(d) != d:
        d = os.path.dirname(d)
    root = d if os.path.exists(os.path.join(d, ".git")) else here
    return [(root, "repo")] + ([(home, "home")] if root != home else [])


def durable_tasks(folder):
    """Claude Code's durable cron tasks in <folder>/.claude/scheduled_tasks.json ({"tasks": [...]}, times in ms), and
    the pid in the folder's scheduler lock, .claude/scheduled_tasks.lock: the session that runs them, if any."""
    def load(name):
        try:
            with open(os.path.join(folder, ".claude", name)) as f:
                data = json.load(f)
        except (OSError, ValueError):
            return {}
        return data if isinstance(data, dict) else {}
    tasks = load("scheduled_tasks.json").get("tasks")
    ok = lambda t: (isinstance(t, dict) and isinstance(t.get("id"), str) and isinstance(t.get("cron"), str) and
                    isinstance(t.get("prompt"), str) and number(t.get("createdAt")))   # Claude Code skips the others
    return ([t for t in tasks if ok(t)] if isinstance(tasks, list) else []), load("scheduled_tasks.lock").get("pid")


def cron_jobs(cfg, marks=None):
    """The CRONS rows: the registry's jobs, then the fleet loops' beats that no job names, then Claude Code's durable
    tasks. A row has name, schedule or every (in seconds), source, machine, last (its last fire, epoch seconds), beat
    (last is its heartbeat's time), once and created (a one-shot task), stopped (why it can't fire), and for a
    heartbeat loop dead (its pid is gone) and ended (the reason its final beat gives). `marks` is what cron_marks read
    on other machines."""
    host, rows, named = platform.node().split(".")[0], [], set()
    for j in cron_registry(cfg):
        machine, beat, pid = str(j.get("machine") or ""), j.get("heartbeat"), j.get("pid")
        here, line = cron_ssh(machine, cfg) is None, None
        if here:
            try:
                seen = os.path.getmtime(os.path.expanduser(beat)) if isinstance(beat, str) else None
                line = beat_of(os.path.expanduser(beat)) if seen is not None else None
            except (OSError, ValueError):
                seen = None
            if line:                         # a loop's own beat: its time and its pid now, not the registry's
                named.add(os.path.realpath(os.path.expanduser(beat)))
                seen, pid = line[0], line[1]
            alive = pid_alive(pid)
        else:   # read on its machine at each data refresh (cron_marks); unknown when that machine didn't answer
            mk = (marks or {}).get(machine) or {}
            seen = mk.get("mtime", {}).get(beat) if isinstance(beat, str) else None
            alive = pid_of(pid) in mk["alive"] if mk and pid_of(pid) else None
        sched = j["schedule"] if isinstance(j.get("schedule"), str) else None
        last = seen if seen is not None else epoch(j.get("last_fire_ts"))
        loop = seen is not None and here     # a far heartbeat is read late: no firing, stale or dead
        ended = line[3] if line else None
        rows.append({"name": j["name"].strip(), "schedule": sched, "every": None if sched else float(j["every_s"]),
                     "source": str(j.get("source") or ""), "machine": machine or host, "last": last,
                     "beat": loop, "once": False, "created": None,
                     "stopped": f"pid {pid} gone" if alive is False and not loop else None,
                     "dead": loop_dead(line, alive) if line and loop else None, "ended": ended})
    folder = os.path.expanduser(cfg.get("loops_dir", LOOPS_DIR))
    try:
        beats = sorted(f for f in os.listdir(folder) if f.endswith(".beat") and LOOP_NAME.fullmatch(f[:-5]))
    except OSError:
        beats = []
    for f in beats:                          # a loop that no job names shows by itself
        path = os.path.join(folder, f)
        if os.path.realpath(path) in named: continue
        line = beat_of(path)
        if not line:                         # a fleet monitor calls such a loop dead, so the view does too
            rows.append({"name": f[:-5], "schedule": None, "every": None, "source": "loop", "machine": host,
                         "last": None, "beat": True, "once": False, "created": None, "stopped": None,
                         "dead": "its beat does not parse", "ended": None})
            continue
        ts, pid, every, ended, _ = line
        rows.append({"name": f[:-5], "schedule": None, "every": every, "source": "loop", "machine": host, "last": ts,
                     "beat": True, "once": False, "created": None, "stopped": None,
                     "dead": loop_dead(line, pid_alive(pid)), "ended": ended})
    for folder, label in durable_folders():
        tasks, lock = durable_tasks(folder)
        running = pid_alive(lock) is True      # only the session that holds the lock runs these tasks
        for t in tasks:
            name = next((l for l in t["prompt"].splitlines() if l.strip()), t["id"])
            rows.append({"name": re.sub(r"\s+", " ", name).strip(), "schedule": t["cron"], "every": None,
                         "source": f"durable · {label}", "machine": host,
                         "last": epoch(t["lastFiredAt"] / 1000) if number(t.get("lastFiredAt")) else None, "beat": False,
                         "once": not t.get("recurring"), "created": epoch(t["createdAt"] / 1000),
                         "stopped": None if running else "no session"})
    return rows


# ── review gaps ────────────────────────────────────────────────────────────────────────────────────────────────────
def scan_gaps_of(scan):
    """The review gaps of a scan state ({"gaps": [...], ...}): the PRs that wait for a review while
    reviewer lanes are idle, as REVIEWERS shows them. [] when it is no scan or names no gap."""
    if not isinstance(scan, dict): return []
    count = lambda v: int(v) if number(v) and v >= 0 else 0
    return [{"pr": norm(g["pr"]), "place": norm(g.get("place")), "what": norm(g.get("what")), "lane": norm(g.get("lane")), "step": norm(g.get("step")),
             "idle": count(g.get("idle")), "reviewers": count(g.get("reviewers"))}
            for g in scan.get("gaps") or [] if isinstance(g, dict) and isinstance(g.get("pr"), str) and g["pr"].strip()]


def scan_gaps(cfg):
    """scan_gaps_of the scan file (`scan_file`, else ~/.fleet/state/scan.json); [] while there is none, or while it
    is not valid JSON (scan writes it by a rename, so that is a file someone else broke)."""
    try:
        with open(os.path.expanduser(cfg.get("scan_file") or SCAN_FILE), encoding="utf-8") as f:
            return scan_gaps_of(json.load(f))
    except (OSError, ValueError):
        return []


# ── reviewers ──────────────────────────────────────────────────────────────────────────────────────────────────────
# A reviewer lane runs a review loop. Each fire scans for a PR, prints
# "LAUNCH <pr> pr-<pr>-user-<account>-<start>-<pid> ...", and runs the review workflow. Claude Code draws a
# running workflow as a row under its prompt: "◯ review-pr  ▰▰▰▰▰▰▰▰▱▱  18/19 · 1h35m · ↓ 2.1m tokens" (█ and ░
# in Ghostty; ⏸ and no bar while a rate limit pauses it). The row stays about 20 s after the run ends, with a full bar.
WF_ROW = re.compile(r"^\s*(?:❯\s*)?([◯⏸]) (\S+)\s{2,}(\S.*)$")
WF_BAR = re.compile(r"(?:([▰▱]+|[█░]+)|(\d{1,3})%)(?:\s{2,}|$)")   # the bar, or its percent where bars can't draw
WF_TIME = re.compile(r"(?:(\d+)d(\d\d)h|(\d+)h(\d\d)m|(\d+)m(\d\d)s|(\d+)s)")
# Text that only a review loop prints, so a pane that quotes it in prose or in a grep is no reviewer: a run id, the
# run's account ref, and the scan's own lines, which start their line.
RUN_ID = re.compile(r"\bpr-(\d{1,7})-user-(\d{1,12})-(\d{9,11})-\d+")     # pr-<pr>-user-<account id>-<start>-<pid>
RUN_REF = re.compile(r"\brefs/review/(\d{1,7})/user/(\d{1,12})\b")       # the run's ref: its PR and account, no start
SCAN = re.compile(r"^\s*(?:[⎿└│]\s*)?# (?:\w+-others|review): ")   # a scan's own lines in shell output: Claude Code ⎿, Codex └, Muse │
PROMPT = re.compile(r"^❯(?:[ \u00a0]|$)")                       # Claude Code's prompt box, and each prompt it echoes
CODEX_PROMPT = re.compile(r"^\s*›(?:[ \u00a0]|$)")              # Codex's prompt, and each prompt it echoes
# Codex draws no workflow row, and it folds a command's output after the first line, so a Codex reviewer often shows
# no run id. Its session's title shows: Herdr's terminal title, and Codex's footer under its prompt
# ("model · ~/worktrees/rv7 · Resume rv7 review loop"). A title that names a review loop
# ("/loop 10m /review", "review-all in a loop") makes such a lane a reviewer. "loop.sh" is a file name.
REVIEW_LOOP_TITLE = re.compile(r"(?i)(?:^|\s)/loop\b.*\breview\w*\b|"
                        r"\breview\w*\b.*\bloop\b(?!\.\w)")
RECENT = 86400   # a run id counts as a reviewer's own only when its run started within the last day
# A head is lowercase hex with a digit, so a word made of the letters a-f ("head: defaced") is not one.
HEAD = re.compile(r"(?i:reviewed-sha=|\bhead_?sha\W{1,4}|\bheadrefoid\W{1,4}|\blive_head\W{1,3}|reviewed at commit\W{1,3}|"
                  r"\bhead(?: commit)?\W{1,3})((?=[0-9a-f]*[0-9])[0-9a-f]{7,40})\b")
# The verdict as a review reports it ("recommendation": "approve", Recommendation: comment), never prose like
# "I posted a comment".
VERDICT = re.compile(r"(?i)\b(?:recommendation|verdict)[\"*]{0,2}\s*[:=]\s*[\"*]{0,2}\s*(approve-with-nits|approve|comment)\b")
POST_FAILED = re.compile(r"(?i)\bpostverified\W{1,4}false\b")
FINISHED = re.compile(r"\bDynamic workflow\b.*\b(?:completed|failed|stopped|killed)\b")
REPO_NAME = r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+"
REPO_PR = re.compile(r"(?:github\.com/|\brepos/)(" + REPO_NAME + r")/pulls?/(\d{1,7})\b")   # a PR's URL or API path
REPO_FLAG = re.compile(r"--repo[ =](" + REPO_NAME + ")")




def lane_run(runs, top):
    """The newest live review run whose session works in this worktree root. The root must match exactly:
    worktrees sit inside the main checkout, so a prefix would give a main-checkout run to every lane."""
    return max((r for r in runs if top and r["anchor"].rstrip("/") == top.rstrip("/")), key=lambda r: r["start"], default=None)


def wf_seconds(s):
    """Seconds in a Claude Code workflow clock (42s, 4m05s, 1h35m, 2d04h), or None."""
    m = WF_TIME.fullmatch(s)
    if not m: return None
    d, dh, h, hm, mi, ms, sec = (int(x) if x else 0 for x in m.groups())
    return (d * 24 + dh + h) * 3600 + (hm + mi) * 60 + ms + sec


def is_review_workflow(name):
    """A workflow row's name is a review workflow's: it starts with "review"."""
    return name.rstrip("…").lower().startswith("review")


def wf_row(line):
    """A Claude Code workflow row, or None: (name, {"frac", "done", "total", "elapsed", "paused", "note"})."""
    m = WF_ROW.match(line)
    if not m: return None
    rest = m.group(3).strip()
    b = WF_BAR.match(rest)
    if not b and m.group(1) != "⏸": return None             # a running row has its bar; only a paused one has none
    segs = [s.strip() for s in (rest[b.end():] if b else rest).split(" · ") if s.strip()]
    row = {"frac": None, "done": None, "total": None, "elapsed": None, "note": None,
           "paused": m.group(1) == "⏸" or "paused" in segs}
    for s in segs:
        n = re.fullmatch(r"(\d+)/(\d+)", s)
        if n and row["total"] is None: row["done"], row["total"] = int(n.group(1)), int(n.group(2))
        elif row["elapsed"] is None: row["elapsed"] = wf_seconds(s)
    if row["total"]: row["frac"] = min(row["done"] / row["total"], 1.0)
    elif b and b.group(1): row["frac"] = sum(c in "▰█" for c in b.group(1)) / len(b.group(1))
    elif b: row["frac"] = min(int(b.group(2)) / 100, 1.0)
    if row["paused"]: row["frac"], row["note"] = None, " · ".join(segs) or "paused"
    return m.group(2), row


def workflow_row(lines):
    """The review workflow row under the pane's prompt box, or None. Claude Code draws its workflow rows below the
    box, so a row-shaped line above it (a tool printed it, say) is only text."""
    tail = [l for l in lines if l.strip()][-20:]
    box = max((i for i, l in enumerate(tail) if PROMPT.match(l)), default=None)
    for ln in reversed(tail[box + 1:] if box is not None else []):
        hit = wf_row(ln)
        if hit and is_review_workflow(hit[0]): return hit[1]
    return None


def claude_code(agent):
    """True for Herdr's name of a Claude Code agent."""
    return (agent or "").lower().startswith("claude")


def loop_titled(agent, title, text):
    """True when a lane that Claude Code does not run titles its session with a review loop: Herdr's terminal title,
    or a line under the last Codex prompt at the pane's foot (Codex's footer). A title that the transcript quotes above
    the prompt does not count. A Claude Code lane draws the workflow row for each review, so its title does not count."""
    if claude_code(agent): return False
    tail = [l for l in (text or "").splitlines() if l.strip()][-12:]
    box = max((i for i, l in enumerate(tail) if CODEX_PROMPT.match(l)), default=None)
    return any(REVIEW_LOOP_TITLE.search(t) for t in [title or ""] + (tail[box + 1:] if box is not None else []))


NO_REVIEW = {"wf": None, "pr": None, "account": None, "start": None, "head": None, "repo": None, "result": False,
             "over": False, "recent": [], "verdict": None, "sign": None}


def repo_named(lines, pr):
    """The repo the pane ties to this PR, the newest mention winning: the PR's URL or API path, or a --repo flag on a
    line with its number. Any other repo in the text (a link to another project, say) is not the PR's."""
    hits = [(i, m.start(), m.group(1)) for i, ln in enumerate(lines) for m in REPO_PR.finditer(ln) if int(m.group(2)) == pr]
    hits += [(i, m.start(), m.group(1)) for i, ln in enumerate(lines) if re.search(rf"\b{pr}\b", ln)
             for m in REPO_FLAG.finditer(ln)]
    return re.sub(r"\.git$", "", max(hits)[2]) if hits else None


def review_of(text, now=None, sure=False):
    """What a pane shows of a review in progress, or None when it shows no reviewer at work: the workflow row, the newest run (its PR,
    account, start, head and repo), whether that run's result is back and whether it is over, the newest runs (repo, PR
    and account), a verdict the session wrote, and what the newest scan said. A pane is a reviewer's when it has the
    workflow row, or a run id that started within the last day; old ids and the scan's lines alone are history, not a
    reviewer. `sure` (the config names the lane a reviewer) reads the pane with no such proof."""
    now = time.time() if now is None else now
    lines = (text or "").splitlines()
    wf = workflow_row(lines)
    runs, scans = [], []                                       # runs: (line, pr, start, account) for each mention
    for i, ln in enumerate(lines):
        if SCAN.match(ln):                                     # the scan lists other sessions' runs: not this lane's
            scans.append(i)
            continue
        runs += [(i, int(m.group(1)), int(m.group(3)), int(m.group(2))) for m in RUN_ID.finditer(ln)]
        for m in RUN_REF.finditer(ln):                         # a ref has no start: it is the run of the newest id
            pr = int(m.group(1))                               # of its PR before it (on the same LAUNCH line)
            runs.append((i, pr, next((r[2] for r in reversed(runs) if r[1] == pr and r[2]), None), int(m.group(2))))
    if not (sure or wf or any(r[2] and now - RECENT < r[2] <= now + 60 for r in runs)): return None
    rv = dict(NO_REVIEW, wf=wf, recent=[])
    begin = -1
    if runs:
        # The newest run is the newest one named outside a `loop.sh free` line (a free names a run that ended). A run is
        # its PR and start, so a new review of the same PR is a new run, with none of the last run's lines.
        newest = next((r for r in reversed(runs) if "loop.sh free" not in lines[r[0]]), runs[-1])
        mine = [r for r in runs if r[1:3] == newest[1:3]]
        begin, rv["pr"], rv["start"] = mine[0][0], newest[1], newest[2]
        rv["account"] = next((r[3] for r in reversed(mine) if r[3]), None)
        tail = lines[begin:]
        heads = [m.group(1) for ln in tail for m in HEAD.finditer(ln)]
        rv["head"], rv["repo"] = (heads or [None])[-1], repo_named(lines, rv["pr"])
        rv["result"] = any(VERDICT.search(ln) or POST_FAILED.search(ln) or FINISHED.search(ln) for ln in tail)
        rv["over"] = any("loop.sh free" in lines[r[0]] for r in mine)   # the lane freed this run's slot
        for _, pr, _, acct in reversed(runs):                  # the two newest runs with an account: where to look
            if acct and len(rv["recent"]) < 2 and (pr, acct) not in [x[1:] for x in rv["recent"]]:   # for its last verdict
                rv["recent"].append((repo_named(lines, pr), pr, acct))
    for i in range(len(lines) - 1, -1, -1):                    # a verdict the session wrote, and the run it belongs to
        m, failed = VERDICT.search(lines[i]), POST_FAILED.search(lines[i])
        if m or failed:
            near = re.search(r'"pr"\s*:\s*(\d{1,7})', lines[i])
            above = [r[1] for r in runs if r[0] <= i]
            rv["verdict"] = {"verdict": "post failed" if failed else m.group(1).lower(),
                             "pr": int(near.group(1)) if near else (above or [None])[-1], "at": None}
            break
    if scans and scans[-1] > begin:                            # a fire after the newest launch (its scan prints first)
        last = lines[scans[-1]]
        rv["over"] = rv["over"] or begin >= 0 and "same-session review in flight" not in last
        if "session parked" in last or "failing closed" in last: rv["sign"] = "blocked"
        elif "SLOTS=0" in last or "another scan is in flight" in last: rv["sign"] = "waiting"
    return rv


def review_status(herdr, rv, run=None):
    """reviewing, posting verdict, waiting, blocked or idle (SKILL.md ▸ Reviewers says what each one means). `run` is
    the lane's live review run, from its lock."""
    wf = rv["wf"]
    if herdr == "blocked" or rv["sign"] == "blocked": return "blocked"
    if wf and wf["paused"]: return "waiting"
    if run and run["written"] or wf and (wf["frac"] or 0) >= 1: return "posting verdict"
    if wf or run: return "reviewing"
    if rv["pr"] and not rv["over"] and herdr == "working": return "posting verdict" if rv["result"] else "reviewing"
    return "waiting" if rv["sign"] == "waiting" else "idle"


def reviewer(l, o, rv, run, read_at):
    """A REVIEWERS row from a lane, what its pane shows and its live run. A lock with no workflow row and no work in
    the pane is left from a run that stopped: it names no PR under review. The pane's head and repo fill in the lock's
    only when the pane's newest run is the lock's run."""
    if run and not (rv["wf"] or l["status"] in ("working", "blocked")): run = None
    st, wf = review_status(l["status"], rv, run), rv["wf"] or {}
    live = st in ("reviewing", "posting verdict") or bool(wf) or bool(run)   # a run is on: its PR is under review
    src = run or rv
    same = not run or rv["pr"] == run["pr"] and rv["start"] in (None, run["start"])
    recent = ([(run["repo"], run["pr"], run["account"])] if run else []) + \
        [x for x in rv["recent"] if not run or x[1:] != (run["pr"], run["account"])]
    elapsed = wf.get("elapsed")
    since = read_at - elapsed if elapsed is not None else src["start"] if live else None
    # Only Claude Code draws a workflow row: a review that runs elsewhere has no bar, and its run is on, not starting.
    note = wf.get("note") or ("running" if st == "reviewing" and not wf and not claude_code(l.get("agent")) else None)
    return {"name": l["name"], "lane": l["lane"], "engagement": o["engagement"], "machine": o["machine"],
            "pane": l.get("pane"), "agent_status": l["status"], "status": st, "live": live,
            "pr": src["pr"] if live else None,
            "repo": ((run or {}).get("repo") or (rv["repo"] if same else None)) if live else None,
            "head": ((run or {}).get("head") or (rv["head"] if same else None)) if live else None,
            "title": None, "url": None, "moved": False, "frac": wf.get("frac"), "done": wf.get("done"),
            "total": wf.get("total"), "elapsed": elapsed, "note": note, "verdict": rv["verdict"],
            "account": src["account"], "recent": recent[:2], "pulse": l.get("pulse", ""), "claim": None,
            "since": since if since and read_at - 7 * 86400 < since <= read_at + 60 else None}


REVIEW_STATE = {"APPROVED": "approve", "COMMENTED": "comment", "CHANGES_REQUESTED": "request changes", "DISMISSED": "dismissed"}


def review_prs(keys):
    """{(repo, number): pr} in one GraphQL call: title, url, state and head commit of each PR, and its newest reviews
    with their state, time, commit and author id (no bodies). None when GitHub does not answer."""
    keys = sorted({(r, n) for r, n in keys if r and re.fullmatch(REPO_NAME, r) and isinstance(n, int)})
    if not keys: return {}
    parts = []
    for i, (repo, num) in enumerate(keys):
        owner, name = repo.split("/")
        parts.append(f'r{i}:repository(owner:"{owner}",name:"{name}"){{pullRequest(number:{num}){{number title url state '
                     'headRefOid repository{name} reviews(last:30){nodes{state submittedAt commit{oid} '
                     'author{... on Bot{databaseId} ... on User{databaseId}}}}}}')
    try:
        data = json.loads(run(["gh", "api", "graphql", "-f", "query={" + " ".join(parts) + "}"], timeout=60))["data"]
    except (ValueError, KeyError, TypeError):
        return None
    if not isinstance(data, dict): return None         # a rate limit answers {"message": ...} with no data
    return {k: (data.get(f"r{i}") or {}).get("pullRequest") for i, k in enumerate(keys) if (data.get(f"r{i}") or {}).get("pullRequest")}


def last_verdict(r, info, default):
    """The newest review the reviewer's account posted on the PRs of its newest runs: {"verdict", "pr", "at"}, or None.
    A run with no known repo is on the fleet's first repo."""
    best = None
    for repo, pr, acct in r["recent"]:
        for n in (((info.get((repo or default, pr)) or {}).get("reviews") or {}).get("nodes") or []):
            who = (n.get("author") or {}).get("databaseId")
            at = n.get("submittedAt") or ""
            if who == acct and n.get("state") in REVIEW_STATE and (best is None or at > best["when"]):
                best = {"verdict": REVIEW_STATE[n["state"]], "pr": pr, "when": at}
    if not best: return None
    try:
        best["at"] = datetime.datetime.strptime(best.pop("when"), "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc).timestamp()
    except ValueError:
        best["at"] = None
    return best


def pool_claims(keys, now=None):
    """No cross-host review claims exist in this copy: always {}."""
    return {}


# ── fixers ─────────────────────────────────────────────────────────────────────────────────────────────────────────
# A reviewer lane fixes what its review found: Reviewer → Fixer → Reviewer. After a review it prints
# "STATUS: fixing #<pr> <head12>", fixes the findings on the PR, prints "STATUS: fixed #<pr> <newhead12>", then
# "STATUS: reviewing", and its review loop goes on. The newest of these three lines in its pane tells whether it
# fixes now. The PR can also come as owner/repo#<pr>, or as repo#<pr> for a repo in `repos`.
FIX_MARK = re.compile(r"STATUS:\s+(?:(fixing|fixed)\s+(?:PR\s*)?(" + REPO_NAME + r"|[A-Za-z0-9_.-]+)?#(\d{1,7})\s+"
                      r"((?=[0-9a-f]*[0-9])[0-9a-f]{7,40})\b|(reviewing)\b)")
OWN_REST = re.compile(r"^  [^\s⎿└│]")              # a later line of a message: two spaces in, and no tool output mark
LAUNCHED = re.compile(r"\bLAUNCH \d{1,7} pr-\d{1,7}-user-|\bWorkflow\([^)]*review")   # a review starts on this line
# A time on the line itself that names its zone: 2026-10-31T14:05:00Z, 2026-10-31 07:05-07:00, 14:05Z or 14:05 UTC.
ZONED = re.compile(r"\b(\d{4})-(\d\d)-(\d\d)[T ](\d\d):(\d\d)(?::(\d\d))?(?:\.\d+)?\s?(Z|UTC|[+-]\d\d:?\d\d)(?!\w)|"
                   r"\b(\d{1,2}):(\d\d)(?::\d\d)?\s?(Z|UTC)\b")
# Claude Code's turn footer, "✻ Worked for 5m 3s · done 2:41 PM": the time the turn ended, in the zone of the lane's
# machine and as the lane's language writes it. "2:41 PM" or "14:41" is on the day Claude Code drew the footer,
# "Friday 12:12" 1 to 6 days before that day, "Thursday, Oct 29, 3:15 PM" 7 days or more before it. Each one reads as
# the newest such moment: the pane keeps a footer as it was drawn, also days later.
DONE = re.compile(r"^\s*[✻✶✢✳✽*·]\s+\S+ for \d[^·]*·\s+done\s+([^·]+?)\s*(?:·|$)")
CLOCK = re.compile(r"(?i)^(?:(mon|tues|wednes|thurs|fri|satur|sun)day,?\s+)?(?:(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|"
                   r"dec)[a-z]*\.?\s+(\d{1,2}),?\s+)?(\d{1,2}):(\d\d)(?:\s*([ap])\.?m\.?)?$")
DAYS = ("mon", "tues", "wednes", "thurs", "fri", "satur", "sun")
MONTHS = ("jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec")
FIX_SEEN = {}          # (machine, pane) -> (pr, head, when a refresh first saw that fixing line); kept from refresh to refresh


def own_lines(lines):
    """[(index, text)] of the lines that the lane wrote: a message after its bullet (● ⏺ • ◆), and each later line of
    that message, two spaces in (Claude Code draws a message's later paragraphs so). A prompt and the text it quotes, a
    tool call and its output, a footer and the prompt box end the message. So a prompt that quotes the rule ("print
    STATUS: fixed #<pr> <newhead12>, then STATUS: reviewing") holds none of the lane's lines."""
    out, mine = [], False
    for i, ln in enumerate(lines):
        m = MESSAGE.match(ln)
        if m:
            body = ln[m.end():].strip()
            mine = not TOOL_LINE.match(body)
            if mine: out.append((i, body))
        elif mine and OWN_REST.match(ln):
            out.append((i, ln.strip()))
        elif ln.strip() and not ln.startswith("   "):    # a deeper line (a nested list, code) goes on with the message
            mine = False
    return out


def last_wall(now, tz, h, mi, wd=None, mon=None, day=None, skip=0):
    """The newest moment at or before `now` (a minute of slack) whose wall clock, `tz` seconds east of UTC, reads h:mi,
    on weekday `wd` (0 is Monday) and on month `mon`, day `day` when they are given. The `skip` days before today are
    left out. None when no day within a year matches."""
    zone = datetime.timezone(datetime.timedelta(seconds=tz))
    today = datetime.datetime.fromtimestamp(now + 60, zone).date()
    for back in range(skip, 400):
        d = today - datetime.timedelta(days=back)
        if wd is not None and d.weekday() != wd or mon and (d.month, d.day) != (mon, day): continue
        t = datetime.datetime(d.year, d.month, d.day, h, mi, tzinfo=zone).timestamp()
        if t <= now + 60: return t
    return None


def pane_time(lines, i, now, tz=None):
    """When the pane says that line i was printed, or None. A time on the line itself counts when it names its zone
    (ZONED). Else the first Claude Code turn footer under the line (DONE) gives the end of a turn that came after it.
    The footer's time is read in the zone of the lane's machine: `tz` seconds east of UTC, or None when not known."""
    m = ZONED.search(lines[i])
    if m and m.group(1):
        y, mo, d, h, mi, s, z = m.group(1, 2, 3, 4, 5, 6, 7)
        off = 0 if z in ("Z", "UTC") else (1 if z[0] == "+" else -1) * (int(z[1:3]) * 3600 + int(z[-2:]) * 60)
        try:
            return datetime.datetime(int(y), int(mo), int(d), int(h), int(mi), int(s or 0),
                                     tzinfo=datetime.timezone(datetime.timedelta(seconds=off))).timestamp()
        except ValueError:
            return None
    if m: return last_wall(now, 0, int(m.group(8)), int(m.group(9))) if int(m.group(8)) < 24 and int(m.group(9)) < 60 else None
    for ln in lines[i + 1:] if tz is not None else []:
        foot = DONE.match(ln)
        if not foot: continue
        c = CLOCK.match(re.sub(r"[\u202f\xa0]", " ", foot.group(1)))   # a narrow no-break space can come before PM
        if not c: return None
        wd, mon, day, h, mi, ap = c.groups()
        h, mi = (int(h) % 12 + (12 if ap.lower() == "p" else 0) if ap else int(h)), int(mi)
        if h > 23 or mi > 59: return None
        return last_wall(now, tz, h, mi, DAYS.index(wd.lower()) if wd else None, MONTHS.index(mon.lower()) + 1 if mon else None,
                         int(day) if day else None, 1 if wd and not mon else 0)   # a weekday alone is not today's
    return None


def fix_of(text, now=None, tz=None):
    """The newest of the lane's own STATUS: fixing, fixed and reviewing lines (own_lines), as {"state", "pr", "repo",
    "head", "at", "after"}, or None when its pane shows none. For a fixing line, `at` is when the pane says that the
    line was printed (pane_time), and `after` is True when a review starts below it (a LAUNCH line, or a review
    workflow)."""
    now = time.time() if now is None else now
    lines = (text or "").splitlines()
    for i, body in reversed(own_lines(lines)):
        m = FIX_MARK.match(body)
        if not m: continue
        fixing = m.group(1) == "fixing"
        return {"state": m.group(1) or m.group(5), "pr": int(m.group(3)) if m.group(3) else None, "repo": m.group(2),
                "head": m.group(4), "at": pane_time(lines, i, now, tz) if fixing else None,
                "after": fixing and any(LAUNCHED.search(ln) and not SCAN.match(ln) for ln in lines[i + 1:])}
    return None


def fix_seen(fx, key, text, now):
    """The lane's fixing line (fx, from fix_of) with `since`: the earlier of the time the pane shows for it and the
    first refresh that saw it. FIX_SEEN keeps that first sight from refresh to refresh, for the same PR and head. A
    pane read whose newest line of the three is not a fixing line drops it. None when the lane does not fix."""
    if not fx or fx["state"] != "fixing":
        if text: FIX_SEEN.pop(key, None)
        return None
    if FIX_SEEN.get(key, (None, None))[:2] != (fx["pr"], fx["head"]): FIX_SEEN[key] = (fx["pr"], fx["head"], now)
    return dict(fx, since=min(t for t in (fx["at"], FIX_SEEN[key][2]) if t is not None))


def fix_repo(fx, lines, rv, run, repos):
    """The repository of the PR that a lane fixes: owner/repo on its line, the repo in `repos` that its line names
    (repo#<pr>), the repo that the pane ties to the PR (repo_named), or the repo of the lane's own review of that PR.
    None when nothing names it: then it is the first repo, as for a review."""
    name = fx["repo"]
    if name and "/" not in name: name = next((r for r in repos or [] if r.split("/")[-1] == name), None)
    reviewed = [(run or {}).get("repo")] if (run or {}).get("pr") == fx["pr"] else []
    return (name or repo_named(lines, fx["pr"]) or
            next((repo for repo in reviewed + [x[0] for x in rv["recent"] if x[1] == fx["pr"]] if repo), None))


def fixer(r, fx):
    """A Fixers row: the REVIEWERS row of a lane that fixes a PR now, with the PR, repo and head of its fixing line and
    `since`, when that line was printed (fix_seen). The review's own fields go: Fixers draws no review."""
    return dict(r, status="fixing", live=False, pr=fx["pr"], repo=fx["repo"], head=fx["head"], since=fx["since"],
                title=None, url=None, moved=False, frac=None, done=None, total=None, elapsed=None, note=None, claim=None)


def local_offset(now):
    """This machine's offset from UTC at `now`, in seconds east of UTC."""
    return int(datetime.datetime.fromtimestamp(now).astimezone().utcoffset().total_seconds())


def split_reviewers(tree, cfg, now=None, prev=None):
    """Move the reviewer lanes out of the tree and return them as REVIEWERS rows. A lane reviews when the config gives it
    the reviewer role, or when it has no role and its pane shows a review run (see review_of) or, outside Claude
    Code, its session's title names a review loop (see loop_titled); the live run in its checkout, from the run
    locks, names the PR. A reviewer lane whose newest line of the three is a fixing line (fix_of) is a Fixers row
    (status `fixing`), unless a live review started after that line. An engagement left with no lane goes. `prev`
    holds the last refresh's rows."""
    out, now = [], time.time() if now is None else now
    known = {r["name"] for r in prev or []}            # the reviewer lanes of the last refresh
    held = {}                                          # run id -> how many lanes work in its checkout
    for l in (l for o in tree for l in o["lanes"] if l.get("run")): held[l["run"]["run"]] = held.get(l["run"]["run"], 0) + 1
    for o in tree:
        keep = []
        for l in o["lanes"]:
            text, role, rk, title = l.pop("text", ""), l.pop("role", None), l.pop("run", None), l.pop("title", "")
            # Lanes that share a checkout share its run: it is the run of the lane whose pane shows the workflow row.
            if rk and held[rk["run"]] > 1 and not workflow_row(text.splitlines()): rk = None
            fx = fix_seen(fix_of(text, now, o.get("tz")) if role in (None, "reviewer") else None, (o["machine"], l.get("pane")),
                          text, now)
            # A lane that fixes shows no workflow row, and its run ids scroll away: it stays a reviewer when the last
            # refresh had it as one, or when its title names a review loop (Claude Code's title too).
            sure = role == "reviewer" or loop_titled(l.get("agent"), title, text) or \
                bool(fx) and (l["name"] in known or bool(REVIEW_LOOP_TITLE.search(title or "")))
            rv = review_of(text, now, sure=sure) if role in (None, "reviewer") else None
            if not rv:
                keep.append(l)
                continue
            r = reviewer(l, o, rv, rk, now)
            # One sub-list per lane: a live review wins over the fixing line only when it started after that line.
            if fx and not (r["live"] and (fx["after"] or (r["since"] or 0) > fx["since"])):
                r = fixer(r, dict(fx, repo=fix_repo(fx, text.splitlines(), rv, rk, cfg.get("repos"))))
            out.append(r)
        o["gone"] = len(keep) < len(o["lanes"]) and not keep
        o["lanes"] = keep
    tree[:] = [o for o in tree if not o.pop("gone")]
    # A long review can push its launch out of the last 160 lines, so the pane is read further back, once per run: a
    # row of the last refresh that started at the same time is the same run, and what that read found stands.
    old, deep = {r["name"]: r for r in prev or [] if r.get("live") and r.get("since")}, []
    for r in (r for r in out if r["live"] and r["pr"] is None and r["pane"]):
        o = old.get(r["name"])
        if o and r["since"] and abs(r["since"] - o["since"]) < 120:
            r.update({k: o[k] for k in ("pr", "repo", "head", "account", "recent")})
        else:
            deep.append(r)
    with cf.ThreadPoolExecutor(max_workers=8) as ex:
        for r, text in zip(deep, ex.map(lambda r: pane_text(r["machine"], r["pane"], lines=1500), deep)):
            rv = review_of(text, now)
            if rv and rv["pr"]:
                r.update(pr=rv["pr"], repo=rv["repo"], head=rv["head"], account=rv["account"], recent=rv["recent"],
                         verdict=r["verdict"] or rv["verdict"], since=r["since"] or rv["start"])
    default = (cfg.get("repos") or [None])[0]
    want = [(r["repo"] or default, r["pr"]) for r in out if r["pr"]] + [(repo or default, pr) for r in out for repo, pr, _ in r["recent"]]
    # In --watch a read of more PRs covers fewer: a reviewer's PR that scrolls out of its pane costs no read.
    info = github("reviews", frozenset(want), lambda: review_prs(want), now, covers=lambda old, new: new <= old) or {}
    for r in out:
        repo = r["repo"] or default
        n = info.get((repo, r["pr"])) if r["pr"] else None
        if n:
            live = n.get("headRefOid") or ""
            r.update(repo=repo, title=n.get("title"), url=n.get("url"), head=r["head"] or live or None,
                     moved=bool(r["head"] and live and not live.startswith(r["head"])))
        r["verdict"] = last_verdict(r, info, default) or r["verdict"]   # GitHub knows; the session's own words are a fallback
        if r["status"] == "fixing" and not r["url"] and repo:            # GitHub did not answer: the link still works
            r["url"] = f"https://github.com/{repo}/pull/{r['pr']}"
    claims = pool_claims([(r["repo"] or default, r["pr"]) for r in out if r["live"] and r["pr"]], now)
    for r in out:
        if r["live"] and r["pr"]: r["claim"] = claims.get((r["repo"] or default, r["pr"]))
    return out


def keep_since(old, new):
    """Carry each running review's start over a data refresh. Claude Code shows an hour-long run to the minute, so a
    fresh read can put the start up to a minute late; the earliest estimate for the same run wins."""
    prev = {(r["name"], r["pr"]): r["since"] for r in old or [] if r.get("since")}
    for r in new:
        s = prev.get((r["name"], r["pr"]))
        if s and r.get("since") and 0 < r["since"] - s < 120: r["since"] = s
    return new


# Removed collection: Herdr lanes only.

def collect(cfg, now=None, prev=None, all_repos=False):
    """Everything the view draws. `prev` is the last refresh's REVIEWERS rows, None on the first read.
    By default only lanes in this checkout's repo show (see lane_repo_matches); --all shows every agent
    on every machine. An engagement left with no lanes is dropped from the tree."""
    orchs = cfg["orchestrators"]
    machines = {o["machine"]: o.get("ssh") for o in orchs}   # Herdr machines only
    here_name = current_repo_basename()  # lanes are scoped to this repo unless show_all
    who = json.dumps([cfg.get("repos"), cfg.get("github_author")], sort_keys=True, default=str)
    # In --watch a search that fails keeps the last good PRs (github); a single read draws what it has (prs).
    read_prs = (lambda: prs(cfg, strict=True)) if GITHUB["on"] else (lambda: prs(cfg))
    with cf.ThreadPoolExecutor(max_workers=8) as ex:
        fa = {m: ex.submit(agents_on, m) for m in machines}
        fp = ex.submit(github, "prs", who, read_prs, now)
        fpin = ex.submit(github, "pins", sorted(set(cfg.get("pins", {}).values())), lambda: pinned(cfg), now)
        fcards = ex.submit(cards_cached, cfg)
        fready = ex.submit(ready_cached, cfg)
        fmarks = ex.submit(cron_marks, cfg)
        ftext = {(o["machine"], pn): ex.submit(pane_text, o["machine"], pn) for o in orchs for pn in panes_of(o)}
        agents = {m: f.result() for m, f in fa.items()}
        dirs = {m: [a["cwd"].replace(" (deleted)", "") for a in (agents[m] or []) if "(deleted)" not in a["cwd"]] for m in machines}
        # review runs are read where the lanes run: on this host, or over ssh (a machine with no alias has none)
        fb = {m: ex.submit(branches_on, machines[m], dirs[m], here=is_here(m, cfg)) for m in machines}
        bres = {m: f.result() for m, f in fb.items()}
        branches = {m: r[0] for m, r in bres.items()}
        mstats = {m: r[1] for m, r in bres.items()}
        pr_map, my_prs = fp.result() or ({}, [])
        pin_map = fpin.result() or {}
        texts = {k: f.result() for k, f in ftext.items()}
        marks = fmarks.result()
        clocks = fready.result()
    for n in list(pr_map.values()) + list(pin_map.values()):   # each open PR's review clock (stall_of)
        n.update(clocks.get(((n.get("repository") or {}).get("name"), n.get("number"))) or {})
    goals, pins, roles = cfg.get("goals", {}), cfg.get("pins", {}), cfg.get("roles", {})
    pulse = cfg.get("pulse", True)
    tree = []
    for o in orchs:
        m, eng = o["machine"], o["engagement"]
        prefix = eng + "-"
        others = [x["engagement"] + "-" for x in orchs if x["machine"] == m and x["engagement"] != eng
                  and (x["engagement"] + "-").startswith(prefix)]
        # A pane started by hand has an agent with no name, so the config names its pane instead (`pane`). Herdr may
        # list no agent there at all; the pane's own text still tells its state.
        hand = panes_of(o)
        found = [a for a in agents[m] or [] if a.get("pane_id") in hand or
                 (((a.get("name") or "").startswith(prefix) or a.get("name") == eng)
                  and not any((a.get("name") or "").startswith(p) for p in others))]
        found += [{"pane_id": pn, "cwd": ""} for pn in hand if pn not in {a.get("pane_id") for a in found}]
        lanes = []
        for a in found:
            name = a.get("name") or ""   # local Herdr also lists unnamed panes (the coordinator's own)
            cwd = a["cwd"].replace(" (deleted)", "")
            if name.startswith(prefix):
                lane = name[len(prefix):]
            elif name and name == eng:
                lane = name
            else:
                lane = pane_lane(eng, cwd, a["pane_id"])
            name = name or prefix + lane
            br = "-" if "(deleted)" in a["cwd"] else branches[m].get(cwd, "-")
            pr = None
            if br not in ("-", "main", "master"):
                hits = [(r, n) for (r, b), n in pr_map.items() if b == br]
                parts = set(re.split(r"[/]", cwd))
                hits.sort(key=lambda h: h[0] not in parts)
                pr = hits[0][1] if hits else None
            if name in pins: pr = pin_map.get(pins[name]) or pr
            goal = goals.get(name) or goals.get(lane) or a.get("terminal_title_stripped") or ""
            l = {"lane": lane, "name": name, "goal": goal.strip(), "branch": br, "pr": pr, "pane": a.get("pane_id"),
                 "status": a.get("agent_status", "unknown"), "agent": a.get("agent", ""),
                 "title": a.get("terminal_title_stripped") or ""}
            role = roles.get(name) or roles.get(lane) or o.get("role")
            if role: l["role"] = role
            rk = lane_run(bres[m][3], bres[m][2].get(cwd))
            if rk: l["run"] = rk
            if l["pane"] in hand:              # one read of the pane gives its state and its newest message
                text = texts.get((m, l["pane"]), "")
                l.update(hand=True, status=pane_status(text, a.get("agent_status")), text=text)
                if pulse and l["status"] != "done": l["pulse"] = pulse_in(text)
            if all_repos or not here_name or lane_repo_matches(cwd, bres[m][2], here_name, l["pane"] in hand):
                lanes.append(l)
        # The machine's UTC offset reads a Claude Code footer's time (pane_time). A machine with no ssh alias has none:
        # its round trip ran on this host.
        tz = local_offset(time.time() if now is None else now) if m == "local" else (mstats.get(m) or {}).get("tz") if machines[m] else None
        if lanes:
            tree.append({"engagement": eng, "machine": m, "label": o.get("label", eng), "stats": mstats.get(m), "repo": o.get("repo"),
                         "project": o.get("project"), "reachable": agents[m] is not None, "lanes": lanes, "tz": tz})
    coord_pulse = coord_ask = ""
    cp = cfg["coordinator"].get("pane")
    if cp:
        # Herdr's blocked is a question or a dialog that waits for the owner. No tick runs until it is answered, so the
        # whole fleet waits: on 2026-11-05 a coordinator dialog stood 3.5 h while this header said 0 asking.
        cs = coordinator_state(cp)
        text = pane_text("local", cp) if pulse or cs and cs[0] == "blocked" else ""
        if pulse: coord_pulse = pulse_in(text)
        if cs and cs[0] == "blocked": cs = coordinator_state(cp)   # read after the text, so an answer in between counts
        coord_ask = coordinator_ask(text, cs)
    # One read of each other pane gives a working lane's newest message and tells whether a lane reviews. A reviewer
    # waits at its prompt between reviews, so a done lane is read too; its pane holds still, so after the first read
    # only a done lane that reviews (by its role, its run lock, its title or the last refresh) is read again. With
    # "pulse": false only those lanes are read.
    known = {r["name"] for r in prev or []}
    jobs = [(l, o["machine"]) for o in tree for l in o["lanes"] if l.get("pane") and not l.get("hand") and
            (l.get("role") == "reviewer" or "run" in l or l["name"] in known or loop_titled(l["agent"], l["title"], "") or
             pulse and (prev is None or l["status"] != "done"))]
    with cf.ThreadPoolExecutor(max_workers=12) as ex:
        for (l, m), text in zip(jobs, ex.map(lambda j: pane_text(j[1], j[0]["pane"]), jobs)):
            l["text"] = text
            if pulse and l["status"] != "done": l["pulse"] = pulse_in(text)
    seen_at = time.time() if now is None else now
    for o in tree:                 # read before split_reviewers takes each lane's text and title
        for l in o["lanes"]: l["health"] = lane_health(l, o["machine"], seen_at)
    reviewers = split_reviewers(tree, cfg, now, prev)
    for o in tree:                 # the tree draws a work lane's health in place of Herdr's status
        for l in o["lanes"]:
            if l.get("health"): l["status"] = l["health"]
            l.pop("health", None)
    ids, links = answered_ids(cfg)
    cards, unreadable = read_decisions(cfg)
    reads = [GITHUB["reads"].get(n) or {} for n in ("prs", "pins", "reviews")]
    gh = {"at": min(c.get("at", 0) for c in reads), "every": GITHUB["every"]} if GITHUB["on"] else None   # github_age
    return {"coordinator": cfg["coordinator"], "orchestrators": tree, "at": time.strftime("%H:%M:%S"), "coord_pulse": coord_pulse,
            "coord_ask": coord_ask,
            "decisions": cards + fcards.result(), "unreadable": unreadable, "answered": ids, "answered_links": links,
            "relays": relays_of(cfg, now),
            "repos": cfg.get("repos", []),
            "my_prs": my_prs, "github_author": cfg.get("github_author", ""),
            "max_prs": cfg.get("max_prs", 20), "hidden": list(cfg.get("hidden", [])), "show_drafts": bool(cfg.get("show_drafts")),
            "repo_order": [r.split("/")[-1] for r in cfg.get("repos", [])], "crons": cron_jobs(cfg, marks), "cron_marks": marks,
            "reviewers": reviewers, "gaps": scan_gaps(cfg), "github": gh}


# ── render ─────────────────────────────────────────────────────────────────────────────────────────────────────────
def pr_cell(p, pr, now=None):
    if not pr: return p("—", C["muted"]), 1
    state = "draft" if pr["isDraft"] and pr["state"] == "OPEN" else pr["state"].lower()
    tag = "" if pr["repository"]["name"] == PRIMARY[0] else pr["repository"]["name"]
    stalled = stall_of(pr, now)                # no approval 3 days after it went ready: red, with its age in days
    num = p.link(p(f"{tag}#{pr['number']}", BOLD, C["fail"] if stalled else C.get(state, C["open"])), pr.get("url"))
    ci = {"SUCCESS": p("✓", C["ok"]), "FAILURE": p("✗", C["fail"]), "ERROR": p("✗", C["fail"]),
          "PENDING": p("…", C["pend"]), "EXPECTED": p("…", C["pend"])}.get(pr["ci"] or "", "")
    if state == "merged": ci = p("merged", C["merged"])
    th = p(f" ⚑{pr['unresolved']}", C["pend"]) if pr["unresolved"] else ""
    age = p(f" {int(stalled // 86400)}d", BOLD, C["fail"]) if stalled else ""
    s = f"{num} {ci}{th}{age}".rstrip()
    return s, width(s)


DIFF_LOOK = {"Accepted": ("✓", "ok"), "Needs Review": ("…", "pend"), "Needs Revision": ("✗", "fail"),
             "Changes Planned": ("✗", "fail"), "Abandoned": ("abandoned", "closed")}


def diff_cell(p, d):
    """A lane's diff in the PR column: the D-number, linked, and its state."""
    if d["landed"] or d["status"] == "Closed": mark, col = "landed", C["merged"]
    else:
        mark, key = DIFF_LOOK.get(d["status"], ("", "draft" if d["status"] == "Draft" else "open"))
        col = C[key]
    num = p.link(p(d["number"], BOLD, C["merged"] if mark == "landed" else C["draft"] if d["status"] == "Draft" else C["open"]), d["url"])
    s = f"{num} {p(mark, col)}".rstrip() if mark else num
    return s, width(s)


PRIMARY = [""]  # the first configured repo's name: its PR numbers show without a repo prefix
SPIN = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
SPARK = "▁▂▃▄▅▆▇█"


TRACK = rgb(56, 62, 92)


def line_bar(p, frac, n, col):
    """Thin bar: heavy line in color, dim line for the rest, with a half step for precision. Without colors the rest
    is a light line, so the bar still reads."""
    frac = max(0.0, min(frac, 1.0))
    full, half = divmod(round(frac * n * 2), 2)
    body = p("━" * full, col) + (p("╸", col) if half else "")
    return body + p(("━" if p.on else "─") * (n - full - half), TRACK)


def gauge(p, frac, n):
    frac = max(0.0, min(frac, 1.0))
    col = rgb(158, 206, 106) if frac < 0.6 else rgb(224, 175, 104) if frac < 0.85 else rgb(247, 118, 142)
    return line_bar(p, frac, n, col)


def github_age(p, data, now):
    """The footer's age of the GitHub data in a --watch view (see github): `GitHub 2m 10s ago` for the oldest of the
    PRs, pins, and reviewer PRs, amber from two periods on (the reads fail) and before the first good read.
    Empty for a single read."""
    gh = data.get("github")
    if not isinstance(gh, dict): return ""
    at, period = gh.get("at") or 0, gh.get("every") or GITHUB_EVERY
    text = f" GitHub {span(now - at)} ago " if at else " no GitHub data yet "
    return p(text, C["pend"] if not at or now - at >= 2 * period else C["muted"])


def span(s):
    """A short duration for a countdown: 42s, 4m 10s, 3h 05m, 2d 4h."""
    s = max(int(s), 0)
    if s < 3600: return f"{s // 60}m {s % 60:02d}s" if s >= 60 else f"{s}s"
    return f"{s // 3600}h {s % 3600 // 60:02d}m" if s < 86400 else f"{s // 86400}d {s % 86400 // 3600}h"


def every_text(s):
    if not s: return "—"                     # a beat that does not parse names no period
    s = int(s)
    return f"every {s // 3600}h" if s % 3600 == 0 else f"every {s // 60}m" if s % 60 == 0 else f"every {s}s"


CRON_LOOK = {  # cron_state -> (glyph, color): a stopped job is dim, and one that can't run is red
    "run": ("↻", C["open"]), "firing": ("↻", C["coord"]), "late": ("↻", C["pend"]), "unseen": ("↻", C["muted"]),
    "stopped": ("◌", STATUS["done"][2]), "bad": ("✗", C["fail"]), "never": ("✗", C["fail"]),
    "stale": ("↻", C["fail"]), "dead": ("✗", C["fail"]),          # a loop that stopped beating, or ended
}
CRON_WAIT = 16  # columns of the countdown: "next in 59m 59s", or a heartbeat's age: "beat 59m 59s ago"


def cron_note(state, j, prev, nxt, now):
    """What a CRONS row says in place of its bar."""
    if state == "stopped": return "stopped · " + j["stopped"]
    if state == "dead" and j.get("ended"):
        return f"dead · ended {span(now - prev)} ago: {j['ended']}" if prev else f"dead · ended: {j['ended']}"
    if state == "dead": return f"dead · {j['dead']}" + (f" · beat {span(now - prev)} ago" if prev else "")
    if state == "stale": return f"stale · beat {span(now - prev)} ago"
    if state == "late": return f"late · due {span(now - nxt)} ago" if nxt else f"late · no fire for {span(now - prev)}"
    return {"bad": "bad schedule", "never": "never fires", "unseen": "no fire seen yet"}[state]


def cron_rows(p, jobs, states, cols, now):
    """One row per job: name, schedule, a bar from its last fire to its next with the percent and a countdown, then
    source @ machine. The bars follow the clock, so every redraw moves them."""
    sched = [j["schedule"] if j.get("schedule") else every_text(j["every"]) for j in jobs]
    wbar = 24 if cols >= 140 else 16 if cols >= 110 else 10
    ws = min(max(width(s) for s in sched), 20) if cols >= 80 else 0      # a narrow pane drops the schedule first
    wmid = wbar + 7 + CRON_WAIT                                          # bar, " 100%", two spaces, countdown
    keep = 14 if cols >= 80 else 0                                       # then source @ machine, before the names
    wn = max(min(max(width(j["name"]) for j in jobs), 24, cols - 6 - (ws + 2 if ws else 0) - wmid - keep), 6)
    out = []
    for j, s, (state, frac, prev, nxt) in zip(jobs, sched, states):
        glyph, col = CRON_LOOK[state]
        if state == "run" and prev is not None and now - prev < 3:
            glyph, col = "✦", C["coord"]                                  # it just fired
        dim = state == "stopped"
        row = (p("│ ", C["frame"]) + p(glyph + " ", col) + p(pad(clip(j["name"], wn), wn), *((C["muted"],) if dim else (BOLD, C["lane"]))) +
               "  " + (p(pad(clip(s, ws), ws), C["muted"] if dim else C["goal"]) + "  " if ws else ""))
        if state in ("run", "firing"):           # a heartbeat shows its age: the loop is alive while it beats
            wait = f"beat {span(now - prev)} ago" if j.get("beat") else "next in " + span(math.ceil(nxt - now))
            row += (line_bar(p, frac, wbar, col) + p(f" {int(frac * 100):>3}%", C["muted"]) + "  " +
                    p(pad(clip(wait, CRON_WAIT), CRON_WAIT), *((BOLD, col) if state == "firing" else (C["goal"],))))
        else:
            row += p(pad(clip(cron_note(state, j, prev, nxt, now), wmid), wmid), col)
        src, machine = j.get("source") or "", j.get("machine") or ""
        who, room = " @ ".join(x for x in (src, machine) if x), cols - width(row) - 2
        if who and room >= width(who) and dim:
            row += "  " + p(who, C["muted"])
        elif who and room >= width(who):
            row += "  " + p(" @ ", C["muted"]).join(([p(src, C["orch"])] if src else []) + ([p(machine, C["machine"])] if machine else []))
        elif who and room >= 6:
            row += "  " + p(clip(who, room), C["muted"])
        out.append(row)
    return out


REVIEW_LOOK = {  # review status -> (glyph, color)
    "reviewing": ("●", STATUS["working"][2]), "posting verdict": ("✎", C["coord"]), "waiting": ("◔", C["pend"]),
    "blocked": ("◆", STATUS["blocked"][2]), "idle": ("○", C["muted"]),
}
VERDICT_LOOK = {"approve": ("approve", C["ok"]), "approve-with-nits": ("approve + nits", C["ok"]),
                "comment": ("comment", C["pend"]), "request changes": ("request changes", C["fail"]),
                "dismissed": ("dismissed", C["muted"]), "post failed": ("post failed", C["fail"])}
REVIEW_NOTE = {"idle": "no review running", "waiting": "waiting for a review slot", "blocked": "needs a person",
               "reviewing": "starting", "posting verdict": "run done"}


def review_clock(r, now):
    """The run's elapsed seconds: a running review follows the clock, a paused or finished one stays where it stopped."""
    if r["status"] == "reviewing" and r.get("since"): return max(now - r["since"], 0)
    return r.get("elapsed")


def same_head(a, b):
    """True when two head prefixes name one commit: the shorter one starts the longer one, and it has 7 digits or more."""
    n = min(len(a or ""), len(b or ""))
    return n >= 7 and a[:n] == b[:n]


def pr_tag(r, cols):
    """repo#number of a row's PR; below 120 columns the primary repository's PRs go without the repo."""
    repo = (r.get("repo") or "").split("/")[-1]
    return f"{repo if repo and (cols >= 120 or repo != PRIMARY[0]) else ''}#{r['pr']}" if r.get("pr") else "—"


def review_rows(p, revs, cols, now, frame, wn=None):
    """One row per reviewer: name, a bar of the run's agents with done/total and the elapsed time, status, then the PR
    under review (repo#number, the head it reviews, its title), the last verdict, the review claim (its host and head)
    and the machine while they fit. `wn` is the width of the names (the Fixers rows' too)."""
    wn = wn or min(max(width(r["name"]) for r in revs), 16)
    wbar = 20 if cols >= 140 else 14 if cols >= 110 else 8
    progs = []
    for r in revs:
        t = review_clock(r, now)
        progs.append(" · ".join(([f"{r['done']}/{r['total']}"] if r.get("total") else []) + ([span(t)] if t is not None else [])))
    wp = max([width(s) for s in progs] + [14])                     # "18/19 · 1h 35m"
    wmid, ws = wbar + 1 + wp, 15                                     # bar and progress; "posting verdict"
    tags = [pr_tag(r, cols) for r in revs]
    heads = [(r.get("head") or "")[:7] for r in revs]
    wc = max(width(t) for t in tags) + (8 if any(heads) else 0)
    verdicts = []
    for r in revs:
        v = r.get("verdict") or {}
        look = VERDICT_LOOK.get(v.get("verdict"))
        text = f"last {look[0]}" + (f" #{v['pr']}" if v.get("pr") else "") + (f" · {ago(v['at'], now)} ago" if v.get("at") else "") \
            if look else ""
        verdicts.append((text, look[1]) if look else None)
    wv = max([width(v[0]) for r, v in zip(revs, verdicts) if v and r.get("pr")] + [0])   # an idle row says it as its title
    wm = max(width("@ " + r["machine"]) for r in revs)
    claims = [(clip(r["claim"]["host"], 16), r["claim"]["head"]) if r.get("claim") else None for r in revs]
    wk = max([width(f"⚐ {c[0]} {c[1][:7]}") for c in claims if c] + [0])
    room = cols - (2 + 2 + wn + 2 + wmid + 2 + ws + 2 + wc) - 1      # what the title, verdict, claim and machine share
    title_room = lambda m, k, v: room - (wm + 2 if m else 0) - (wk + 2 if k else 0) - (wv + 2 if v else 0) - 2
    show_m, show_k, show_v = True, bool(wk), bool(wv)
    if title_room(show_m, show_k, show_v) < 16: show_m = False      # a narrow pane drops the machine first,
    if title_room(show_m, show_k, show_v) < 16: show_k = False      # then the claim, then the last verdict,
    if title_room(show_m, show_k, show_v) < 16: show_v = False      # then it cuts the title
    wt = title_room(show_m, show_k, show_v)
    out = []
    for i, (r, prog, tag, head, v, k) in enumerate(zip(revs, progs, tags, heads, verdicts, claims)):
        glyph, col = REVIEW_LOOK[r["status"]]
        if r["status"] == "reviewing" and p.on: glyph = SPIN[(frame + i * 3) % len(SPIN)]
        row = p("│ ", C["frame"]) + p(glyph + " ", col) + p(pad(clip(r["name"], wn), wn), BOLD, C["lane"]) + "  "
        if r.get("frac") is not None:
            row += line_bar(p, r["frac"], wbar, col) + " " + p(pad(prog, wp), C["goal"])
        else:
            note = r.get("note") or REVIEW_NOTE[r["status"]]
            row += p(pad(clip(note + (f" · {prog}" if prog and r["live"] else ""), wmid), wmid), C["muted"])
        row += "  " + p(pad(r["status"], ws), BOLD if r["status"] in ("reviewing", "blocked") else "", col) + "  "
        cell = p.link(p(tag, BOLD, C["open"]), r.get("url")) if r.get("pr") else p(tag, C["muted"])
        if head: cell += " " + p(head, C["pend"] if r.get("moved") else C["muted"])   # amber: the PR moved past it
        row += cell + " " * max(wc - width(tag) - (1 + width(head) if head else 0), 0)
        if wt >= 6 and not r.get("pr") and v: row += "  " + p(pad(clip(v[0], wt), wt), v[1])
        elif wt >= 6: row += "  " + p(pad(clip(r.get("title") or "", wt), wt), C["goal"])
        if show_v: row += "  " + (p(pad(v[0], wv), v[1]) if v and r.get("pr") else " " * wv)
        if show_k:                                                  # amber: the claim names another head
            cell = p("⚐ ", C["muted"]) + p(k[0], C["machine"]) + " " + \
                p(k[1][:7], C["muted"] if same_head(r.get("head") or k[1], k[1]) else C["pend"]) if k else ""
            row += "  " + pad(cell, wk)
        if show_m: row += "  " + p("@ ", C["muted"]) + p(r["machine"], C["machine"])
        out.append(row.rstrip())
    return out


FIX_LOOK = ("⚒", rgb(255, 158, 100))   # a fixer: orange, and it spins in --watch while its lane works


def fix_rows(p, fixers, cols, now, frame, wn=None):
    """One row per fixer: name, the PR it fixes (repo#number, a link), the head it fixes, the time since its fixing
    line, the status `fixing`, and the machine while it fits (a narrow pane drops the machine first, then the row is
    cut). The head is amber when the PR moved past it. `wn` is the width of the names (the Reviewers rows' too)."""
    wn = wn or min(max(width(r["name"]) for r in fixers), 16)
    tags = [pr_tag(r, cols) for r in fixers]
    clocks = [span(now - r["since"]) if r.get("since") else "—" for r in fixers]   # it follows the clock in --watch
    wt, wc = max(width(t) for t in tags), max(width(c) for c in clocks)
    wm = max(width("@ " + r["machine"]) for r in fixers)
    show_m = 2 + 2 + wn + 2 + wt + 8 + 2 + wc + 2 + 6 + 2 + wm <= cols - 1
    out = []
    for i, (r, tag, clock) in enumerate(zip(fixers, tags, clocks)):
        glyph, col = FIX_LOOK
        if r["agent_status"] == "working" and p.on: glyph = SPIN[(frame + i * 3) % len(SPIN)]
        cell = p.link(p(tag, BOLD, C["open"]), r.get("url")) if r.get("pr") else p(tag, C["muted"])
        head = (r.get("head") or "")[:7]
        row = (p("│ ", C["frame"]) + p(glyph + " ", col) + p(pad(clip(r["name"], wn), wn), BOLD, C["lane"]) + "  " + pad(cell, wt) +
               " " + p(pad(head, 7), C["pend"] if r.get("moved") else C["muted"]) + "  " + p(pad(clock, wc), C["goal"]) +
               "  " + p("fixing", BOLD, col))
        if show_m: row += "  " + p("@ ", C["muted"]) + p(r["machine"], C["machine"])
        out.append(row)
    return out


RAINBOW = [(255, 59, 48), (255, 159, 10), (255, 214, 10), (52, 199, 89), (10, 132, 255), (175, 82, 222)]


TAIL = 18  # rainbow length behind the cat, in columns


def nyan(p, cols, frame):
    """Two quiet lines: a tiny Nyan Cat flying across the pane on a six-band rainbow (three bands per line)."""
    soft = lambda c, k=0.72: tuple(int(v * k) for v in c)   # muted so it stays in the background
    cat = [",--,/\\_/\\", "'--'(^.^) "]
    cw = width(cat[0])
    span = max(cols - cw - 2, 10)
    x = (frame // 2) % span
    out = []
    for line in range(2):
        row = [" "] * max(x - TAIL, 0)
        for c in range(max(x - TAIL, 0), x):
            shimmer = ((c - frame) // 3) % 2               # every 3 columns the trail dims a little: a slow ripple
            k = 0.72 if shimmer else 0.6
            if p.on:
                top, mid, bot = (RAINBOW[3 * line + i] for i in range(3))
                # three bands in one cell row: upper half = top band, lower half = bottom band, middle band blended
                up, low = (top, mid) if c % 2 else (mid, bot)
                row.append(rgb(*soft(up, k)) + bg(*soft(low, k)) + "▀" + RESET)
            else:
                row.append("=" if line == 0 else "-")
        body = cat[line]
        if p.on:
            body = p(body[:4], rgb(*soft((255, 153, 204), 0.85))) + p(body[4:], rgb(*soft((210, 210, 220), 0.85)))
        out.append("".join(row) + body)
    return out


GAP_ROWS = 5       # the PRs of a review gap that REVIEWERS lists; the scan's gap lines name the rest (#9099)
# FOLD_KEYS is the canonical section order (top to bottom) and the fallback
# key mapping before the first frame is drawn. A header's number is assigned
# in draw order by render() (see fold_mark): only sections actually drawn
# get numbers, so a hidden section leaves no gap in the visible numbering.
FOLD_KEYS = {"1": "tree", "2": "reviewers", "3": "decisions", "4": "crons", "5": "prs"}
AUTO_FOLD = ("prs", "crons", "reviewers", "decisions")  # folded in this order when a pane is too short; lanes never


def render(data, plain=False, show_all=False, frame=0, dense=False, links=True, fun=False, interactive=False, collapsed=(), hidden=(),
           now=None, decide=None, numbering=None):
    p = Paint(not plain, links)
    now = time.time() if now is None else now    # the CRONS bars follow this clock, not the data
    PRIMARY[0] = (data.get("repo_order") or [""])[0]
    cols = shutil.get_terminal_size((140, 40)).columns
    lines = []
    numbered = {}
    def fold_mark(key):
        # Numbers follow draw order, not FOLD_KEYS: a section that is not
        # drawn (no reviewers, no cron jobs) takes no number, so the visible
        # headers read 1, 2, 3, ... with no gaps. The mapping for this frame
        # is reported back through `numbering` for key presses.
        if key not in numbered: numbered[key] = str(len(numbered) + 1)
        if numbering is not None: numbering.clear(); numbering.update({v: k for k, v in numbered.items()})
        return f" {'▸' if key in collapsed else '▾'}{numbered[key]}" if interactive else ""
    def section(key, title, tail="", tail_color=None):
        """Draw a section header; False when the section is folded and its body must be skipped."""
        if not dense: lines.append(p("│", C["frame"]))
        title = fold_mark(key) + title
        head = p.link(p.gradient(title), "fleetview:fold:" + key) if interactive else p.gradient(title)  # click folds
        lines.append(p("├─", C["frame"]) + head + p("─" * max(cols - width(title) - width(tail) - 4, 2), C["frame"]) +
                     (p(tail, tail_color or C["muted"]) if tail else "") + p("─┤", C["frame"]))
        if key in collapsed: return False
        if not dense: lines.append(p("│", C["frame"]))
        return True
    co = data["coordinator"]
    # The reviewer lanes are lanes too: the header and the status counts take them in, a blocked reviewer included (it
    # asks). The lanes that ask and the Codex queues that wait a tick (#7807) have their own counts.
    every = [lane_status(l["status"]) for o in data["orchestrators"] for l in o["lanes"]]
    every += [lane_status(r["agent_status"]) for r in data.get("reviewers") or []]
    total, active = len(every), sum(1 for s in every if s != "done")
    asking = every.count("asking") + (1 if data.get("coord_ask") else 0)   # a coordinator that asks counts too
    stats = f" {active} active · {asking} asking · {every.count('queued')} queued · {total} lanes · {data['at']} "
    lanes_mark = f"·{fold_mark('tree')} lanes " if interactive else ""
    # A long engagement list must not wrap the header: one extra row scrolls the whole watch screen every frame.
    room = max(cols - width(stats) - width(lanes_mark) - width(" ⚓ FLEET ·  ") - 10, 8)
    title = f" ⚓ FLEET · {clip(co.get('engagement', ''), room)} " + lanes_mark
    bar = max(cols - width(title) - width(stats) - 4, 4)
    lines.append(p("╭─", C["frame"]) + (p.link(p.gradient(title), "fleetview:fold:tree") if interactive else p.gradient(title)) + p("─" * bar, C["frame"]) + p(stats, C["muted"]) + p("─╮", C["frame"]))
    counts = {"asking": 1} if data.get("coord_ask") else {}   # the same asking count as the header
    for s in every: counts[s] = counts.get(s, 0) + 1
    sb = "  ".join(p(f"{STATUS[k][0]} {counts.get(k, 0)}", BOLD if k in ("working", "asking", "queued") and counts.get(k) else "", STATUS[k][2])
                   for k in ("working", "asking", "queued", "idle", "done"))
    coord = (p("│ ", C["frame"]) + p("◉ ", C["coord"]) + p("Coordinator", BOLD, C["coord"]) + "  " +
             p(co.get("name", "operator"), C["lane"]) + p("  @ ", C["muted"]) + p(co.get("machine", "?"), BOLD, C["machine"]))
    lines.append(pad(coord, cols - width(sb) - 3) + sb)
    if data.get("coord_ask"):
        ask = STATUS["asking"]
        lines.append(p("│ ", C["frame"]) + p(f"  {ask[0]} waiting for you: ", BOLD, ask[2]) +
                     p(clip(re.sub(r"\s+", " ", data["coord_ask"]), cols - 24), BOLD, ask[2]))
    if data.get("coord_pulse"):
        lines.append(p("│ ", C["frame"]) + p("  ❯ ", C["coord"]) + p(clip(re.sub(r"\s+", " ", data["coord_pulse"]), cols - 8), ITAL, C["goal"]))
    orchs = data["orchestrators"]
    shown = [l for o in orchs for l in o["lanes"] if show_all or l["status"] != "done"] or [{"lane": "", "branch": ""}]
    wl = max(width(l["lane"]) for l in shown)
    wb = min(max(width(l["branch"]) for l in shown), 30 if cols >= 140 else 20 if cols >= 100 else 0)
    pw = max([14] + [diff_cell(p, l["diff"])[1] + 1 for l in shown if l.get("diff")])   # a landed diff is 17 wide
    grouped = any(o.get("project") or o.get("repo") for o in orchs)   # one more level: coordinator > group > lane > worker
    wg = max(cols - (2 + 3 + 3 + 2 + wl + 2 + (2 + wb + 2 if wb else 0) + pw + 2 + 11 + 2) - 4 - (3 if interactive else 0) - (3 if grouped else 0), 12)
    gone = set(data.get("hidden") or ()) | set(hidden)   # lanes removed with ✕: hidden while they stay done
    def visible(o): return [l for l in o["lanes"] if l["status"] != "done" or (show_all and l.get("name") not in gone)]
    quiet = [o for o in orchs if not visible(o)]
    orchs = [o for o in orchs if visible(o)]
    seen_hosts = set()
    groups = []                    # [(key, [orchs])] in first-seen order: the engagement's project, else its repo
    for o in orchs:
        key = ("project", o["project"]) if o.get("project") else ("repo", o.get("repo") or "")
        hit = next((g for g in groups if g[0] == key), None)
        if hit: hit[1].append(o)
        else: groups.append((key, [o]))
    flat = []                                     # (orch, its index within the group, group size, group index)
    for gi, (key, members) in enumerate(groups):
        for i, o in enumerate(members): flat.append((o, i, len(members), gi))
    places = []                                   # (repo, machine) of each heading drawn, in order
    for oi, (o, gpos, gsize, gi) in enumerate(flat):
        last_g = gi == len(groups) - 1 and not quiet
        if grouped and gpos == 0:
            key, members = groups[gi]
            gl = [l for m in members for l in visible(m)]
            busy = sum(1 for l in gl if l["status"] == "working")
            if not dense and "tree" not in collapsed:
                lines.append(p("│ ", C["frame"]) + p("  │", C["frame"]))
            lines.append(p("│ ", C["frame"]) + p("  " + ("└─" if last_g else "├─") + " ", C["frame"]) + p("▣ ", C["coord"]) +
                         p(key[1] or "other", BOLD, C["coord"]) + p(f"  {len(members)} lanes · {len(gl)} workers · {busy} working", C["muted"]))
        pre = ("   " if last_g else "│  ") if grouped else ""
        last_o = (gpos == gsize - 1) if grouped else (oi == len(flat) - 1 and not quiet)
        lanes = visible(o)
        hidden = len(o["lanes"]) - len(lanes)
        branch = pre + ("└─" if last_o else "├─")
        stem = pre + ("   " if last_o else "│  ")
        reach = "" if o["reachable"] else p("  ⚠ unreachable", BOLD, C["fail"])
        counts = p(f"  {hidden} done hidden", C["muted"]) if hidden else ""
        if not dense and "tree" not in collapsed and not (grouped and gpos == 0):
            lines.append(p("│ ", C["frame"]) + p("  " + pre + "│", C["frame"]))
        lead = p("│ ", C["frame"]) + p("  " + branch + " ", C["frame"]) + p("◈ ", C["orch"])
        # A host's cpu and mem bars show once, on its first row. A later row on that host keeps only its name, dimmed.
        first = o["machine"] not in seen_hosts
        seen_hosts.add(o["machine"])
        # The repository (a project spans repos) and the machine show on the first heading of a group, and on a later
        # one only when they differ from the heading drawn above it in the same group.
        repo = o["repo"] if o.get("project") and o.get("repo") else ""
        place, before = (repo, o["machine"]), (places[-1] if gpos else None)
        places.append(place)
        mname = o["machine"]
        where = (p(f"  {repo}", C["muted"]) if repo else "") + p("  @ ", C["muted"]) + \
            p(mname, *((BOLD, C["machine"]) if first else (C["muted"],))) if place != before else ""
        tail = where + counts + reach
        st = o.get("stats") if first else None
        right = ""
        if st and "load" in st and cols >= 100:   # a Mac has no load or memory reading in its stats
            right = (p("cpu ", C["muted"]) + gauge(p, st["load"] / max(st["cores"], 1), 10) + p(f" {st['load']:.0f}/{st['cores']}", C["muted"]) +
                     p("  mem ", C["muted"]) + gauge(p, st["mem_used"] / max(st["mem_total"], 1), 10) +
                     p(f" {st['mem_used'] / 1024:.0f}/{st['mem_total'] / 1024:.0f}G", C["muted"]))
        # The heading never wraps: a wrapped line makes the watch frame taller than the pane, and the pane then scrolls
        # on every redraw (it shakes). The cpu and mem bars go first when they can't fit, then the label is clipped.
        room = cols - width(lead) - width(tail) - 1
        if right and room - width(right) - 2 < 12:
            right = ""
        label = clip(o["label"], max(room - (width(right) + 2 if right else 0), 8))
        head = lead + p(label, BOLD, C["orch"]) + tail
        if right:
            head = pad(head, cols - width(right) - 2) + right
        lines.append(head)
        if not lanes or "tree" in collapsed: continue
        for li, l in enumerate(lanes):
            lb = "└─" if li == len(lanes) - 1 else "├─"
            st = lane_status(l["status"])
            g, label, color = STATUS.get(st, STATUS["unknown"])
            if st == "working" and not plain:
                g = SPIN[(frame + li * 3 + oi * 5) % len(SPIN)]
            prs_, prw = diff_cell(p, l["diff"]) if l.get("diff") else pr_cell(p, l["pr"], now)
            goal = l["goal"] or "—"
            row = (p("│ ", C["frame"]) + p("  " + stem + lb + " ", C["frame"]) +
                   p(pad(l["lane"], wl), BOLD, C["lane"]) + p("  ", "") +
                   p(pad(clip(goal, wg), wg), C["goal"] if l["goal"] else C["muted"]) + p("  ", "") +
                   (p(" " + pad(clip(l["branch"], wb), wb), C["branch"] if l["branch"] != "-" else C["muted"]) + "  " if wb else "") +
                   prs_ + " " * max(pw - prw, 1) + p("  ", "") +
                   p(f"{g} {label}", BOLD if st in ("working", "asking", "queued") else "", color))
            if interactive and l["status"] == "done" and l.get("name"):
                row = pad(row, cols - 3) + p.link(p("✕", BOLD, C["fail"]), "fleetview:hide:" + l["name"])  # click to remove
            lines.append(row)
    if quiet:
        if not dense and "tree" not in collapsed: lines.append(p("│ ", C["frame"]) + p("  │", C["frame"]))
        names = "  ".join(p(o["label"].split(" ·")[0], C["orch"]) + p(f" {len(o['lanes'])}✓", C["muted"]) for o in quiet)
        lines.append(p("│ ", C["frame"]) + p("  └─ ", C["frame"]) + p("◇ ", C["muted"]) + p("idle  ", ITAL, C["muted"]) + names)
    # REVIEWERS · FIXERS (#7807): a reviewer lane fixes what its review found. Reviewers draws the reviewers with a
    # review on (live) and the blocked ones: a blocked reviewer needs a person. Fixers draws each reviewer lane that
    # fixes a PR now (status fixing). An idle or waiting reviewer with no run on is drawn in neither, and it stays out
    # of the tree too: only the tally counts it. With no row to draw the section is not drawn. A review gap of the last
    # scan (#9099: reviewer lanes idle while PRs wait for a review) comes first, and it draws the section on its own.
    revs = data.get("reviewers") or []
    fixers = [r for r in revs if r["status"] == "fixing"]
    shown = [r for r in revs if r["status"] != "fixing" and (r["live"] or r["status"] == "blocked")]
    gaps = data.get("gaps") or []
    tally = [f"{len(revs)} reviewer{'s' * (len(revs) != 1)}"] + [f"{n} {st}" for st in ("reviewing", "posting verdict", "blocked", "fixing")
                                                                 if (n := sum(1 for r in revs if r["status"] == st))]
    if len(revs) > len(shown) + len(fixers): tally.append(f"{len(revs) - len(shown) - len(fixers)} idle hidden")
    if gaps: tally.append(f"{len(gaps)} PR{'s' * (len(gaps) != 1)} wait for a review")
    if (shown or fixers or gaps) and section("reviewers", " REVIEWERS · FIXERS ", f" {' · '.join(tally)} "):
        if gaps:
            # The scan counts each repo's reviewer lanes on its own, so each repo of the gaps gets its own header.
            repos = {}
            for g in gaps: repos.setdefault(g.get("place", ""), []).append(g)
            left = GAP_ROWS
            for repo, group in repos.items():
                head = (f"{repo}: " if len(repos) > 1 and repo else "") + \
                       f"{group[0]['idle']} of {group[0]['reviewers']} reviewer lanes were idle at the last scan while " \
                       f"{len(group)} PR{'s wait' if len(group) != 1 else ' waits'} for a review"
                lines.append(p("│ ", C["frame"]) + p("⚠ ", BOLD, C["pend"]) + p(clip(head, cols - 6), BOLD, C["pend"]))
                for g in group[:max(left, 0)]:
                    text = f"{g['pr']}  {g['what']}" + (f" · {g['lane']}" if g["lane"] else "") + f" → {g['step']}"
                    lines.append(p("│ ", C["frame"]) + p("    " + clip(text, cols - 8), C["goal"]))
                left -= len(group)
            if len(gaps) > GAP_ROWS:
                lines.append(p("│ ", C["frame"]) + p(f"    +{len(gaps) - GAP_ROWS} more: the scan's gap lines name them", ITAL, C["muted"]))
        wn = min(max((width(r["name"]) for r in shown + fixers), default=8), 16)     # one name column for both sub-lists
        for name, rows in (("Reviewers", shown and review_rows(p, shown, cols, now, frame, wn)),
                           ("Fixers", fixers and fix_rows(p, fixers, cols, now, frame, wn))):
            if rows: lines += [p("│ ", C["frame"]) + p("▸ ", C["muted"]) + p(name, BOLD, C["goal"])] + rows
    dm = decide or {}                              # the decision mode of --watch: on, the picked decision and option
    asks = open_asks(data, dm.get("gone", ()))
    hint = (" 1-9 pick · a-z ←→ option · ↵ send · Esc back " if dm.get("on") else " d answers ") if interactive and asks else ""
    unreadable = data.get("unreadable") or []
    folded = f" {len(asks)} open " + (f"· {len(unreadable)} unreadable " if unreadable else "")
    if section("decisions", " DECISIONS · waiting on you ", folded if "decisions" in collapsed else hint):
        if not asks and not unreadable:
            lines.append(p("│ ", C["frame"]) + p("  ✓ nothing waiting on you", C["ok"]))
        for u in unreadable:                       # an entry the view cannot show may still wait on the owner (#8946)
            text = f"decisions file: {u['why']} — the coordinator rewrites it as {{q, options, recommend, lane, url}}"
            lines.append(p("│ ", C["frame"]) + p("  ⚠ ", BOLD, C["pend"]) + p(clip(text, cols - 8), C["pend"]))
        for i, d in enumerate(asks, 1):
            sel = bool(dm.get("on")) and dm.get("sel") == d["id"]
            tag = f"  {d['lane']}" if d["lane"] else ""
            num = (("▸" if sel else " ") if interactive else "") + f"{i:>2}. "
            badge = "Needs your label · " if d["kind"] == "label" else ""
            at = d.get("relayed")                      # when the lane's line reached the coordinator's pane
            relayed = f"↪ relayed {'just now' if now - at < 60 else ago(at, now) + ' ago'} · " if at else ""
            q = p(clip(d["q"], cols - width(tag) - width(num) - width(relayed) - width(badge) - 5), BOLD, C["lane"])
            head = p(num, BOLD, C["coord"] if sel else C["fail"])
            lines.append(p("│ ", C["frame"]) + (p.link(head, f"fleetview:ask:{d['id']}") if interactive else head) +   # click picks it
                         p(relayed, C["coord"]) + p(badge, BOLD, C["pend"]) + (p.link(q, d["url"]) if d["url"] else q) +
                         p(tag, C["orch"]))
            opts = d["options"]
            one = len(opts) == 1 and rec_index(opts, d.get("recommend")) == 0   # the recommendation is the only option
            cells = []
            for j, o in enumerate(opts):
                key, text = (f"{chr(97 + j)} " if interactive and j < 26 else ""), o + (" ★" if one else "")
                if sel and dm.get("opt") == j:
                    cell = p(key + text, "\x1b[7m", BOLD, C["coord"]) if p.on else f"[{key}{text}]"
                else:
                    cell = p(key, BOLD, C["coord"]) + p(text, C["muted"])
                cells.append(p.link(cell, f"fleetview:answer:{d['id']}:{j}") if interactive else cell)   # click picks, again sends
            if not cells and interactive:          # nothing to pick here: the owner answers the coordinator directly
                cells = [p("no options to pick: answer in the coordinator's pane", ITAL, C["muted"])]
            rec = f"  ★ {d['recommend']}" if d.get("recommend") and not one and d["kind"] == "choice" else ""
            go = ("  ↵ " + ("send" if d["kind"] == "choice" else "open")) if sel and dm.get("opt") is not None else ""
            if cells or rec:
                room, body = max(cols - width(rec) - width(go) - 9 - len(num) + 4, 8), p(" · ", C["muted"]).join(cells)
                lines.append(p("│ ", C["frame"]) + " " * (len(num)) + (fit_line(body, room) if p.on else clip(body, room)) +
                             p(rec, C["ok"]) + p(go, BOLD, C["coord"]))
    jobs = data.get("crons") or []
    states = [cron_state(j, now) for j in jobs]
    soon = min(((st[3], j["name"]) for j, st in zip(jobs, states) if st[0] == "run"), default=None)
    down = "".join(f" · {n} {k}" for k in ("dead", "stale") for n in [sum(st[0] == k for st in states)] if n)
    tail = f" {len(jobs)} jobs{down}" + (f" · next {soon[1]} in {span(math.ceil(soon[0] - now))}" if soon else "") + " "
    if jobs and section("crons", " CRONS · scheduled jobs ", tail if "crons" in collapsed else ""):
        lines += cron_rows(p, jobs, states, cols, now)
    mine = data.get("my_prs") or []
    n_open = sum(1 for n in mine if n["state"] == "OPEN")
    prs_open = section("prs", " YOUR OPEN PRS ", f" {n_open} open " if "prs" in collapsed else "")
    palette = [rgb(115, 218, 202), rgb(187, 154, 247), rgb(255, 158, 100), rgb(122, 162, 247)]
    order = data.get("repo_order") or sorted({n["repository"]["name"] for n in mine})
    colmap = {repo: palette[i % len(palette)] for i, repo in enumerate(order)}
    def merged_ts(n):
        return time.mktime(time.strptime(n["mergedAt"], "%Y-%m-%dT%H:%M:%SZ")) - time.timezone if n.get("mergedAt") else 0
    budget = int(data.get("max_prs", 20))
    for ri, repo in enumerate(order if prs_open else []):
        opened = sorted([n for n in mine if n["repository"]["name"] == repo and n["state"] == "OPEN"], key=lambda n: -n["number"])
        drafts = 0 if data.get("show_drafts") else sum(1 for n in opened if n["isDraft"])
        if drafts:                                    # drafts are not ready for anyone: hidden unless show_drafts
            opened = [n for n in opened if not n["isDraft"]]
        col = colmap.get(repo, C["orch"])
        show = opened[:max(budget, 0)]
        budget -= len(show)
        more = len(opened) - len(show)
        lines.append(p("│ ", C["frame"]) + p("▸ ", col) + p(repo, BOLD, col) + p(f"   {len(opened)} open", C["muted"]) +
                     (p(f" · {more} more not shown", C["muted"]) if more else "") +
                     (p(f" · {drafts} draft{'s' * (drafts != 1)} hidden", C["muted"]) if drafts else ""))
        if not show:
            lines.append(p("│ ", C["frame"]) + p("   └─ ", C["frame"]) + p("no open PRs" if not opened else "over the limit", ITAL, C["muted"]))
        for i, n in enumerate(show):
            last = i == len(show) - 1
            cell, cw_ = pr_cell(p, n, now)
            kind = p(" draft", C["draft"]) if n["isDraft"] else p("  open", C["open"])
            lines.append(p("│ ", C["frame"]) + p("   " + ("└─ " if last else "├─ "), C["frame"]) + kind + "  " +
                         cell + " " * max(16 - cw_, 1) + p(clip(n.get("title") or "", cols - 36), C["goal"]))
        if ri < len(order) - 1 and not dense:
            lines.append(p("│", C["frame"]))
    if not dense: lines.append(p("│", C["frame"]))
    legend = (p("●", STATUS["working"][2]) + " working  " + p("⁇", STATUS["asking"][2]) + " asking  " +
              p("↳", STATUS["queued"][2]) + " queued  " + p("○", STATUS["idle"][2]) + " idle  " + p("✓", STATUS["done"][2]) + " done   " +
              p("#PR", C["open"]) + " open " + p("#PR", C["merged"]) + " merged " + p("#PR", C["draft"]) + " draft " +
              p("#PR 3d", C["fail"]) + " unapproved   " +
              p("✓", C["ok"]) + "/" + p("✗", C["fail"]) + "/" + p("…", C["pend"]) + " CI   " + p("⚑n", C["pend"]) + " open threads")
    age = github_age(p, data, now)
    if width(legend) + width(age) > cols - 8:
        legend = (p("●", STATUS["working"][2]) + " work " + p("⁇", STATUS["asking"][2]) + " asking " +
                  p("↳", STATUS["queued"][2]) + " queued " + p("✓", STATUS["done"][2]) + " done  " +
                  p("✓", C["ok"]) + p("✗", C["fail"]) + " CI " + p("⚑", C["pend"]) + " threads")
    lines.append(p("╰─ ", C["frame"]) + legend + p(" " + "─" * max(cols - width(legend) - width(age) - 7, 2), C["frame"]) + age +
                 p("─╯", C["frame"]))
    if fun:
        rows = shutil.get_terminal_size((cols, 40)).lines
        lines = lines[:max(rows - 2, 1)]
        lines += [""] * max(rows - 2 - len(lines), 0)   # the cat flies on the pane's last two rows
        lines += nyan(p, cols, frame)
    return "\n".join(lines)


def fit(data, rows, view, frame=0, show_all=False, links=True, fun=False, plain=False, now=None):
    """Lines for a pane `rows` tall. First drop spacer lines, then fold sections in AUTO_FOLD order (never one the
    user set with a key), then scroll. `view` holds the user's folds and the scroll offset; fit updates it. In the
    decision mode DECISIONS stays open and a newly picked decision scrolls into the pane; a toast (a sent answer)
    takes the last row for its few seconds."""
    folds = view.setdefault("folds", {})
    avail = max(rows - (2 if fun else 0), 3)
    if view.get("unhide_all"): data = dict(data, hidden=[])   # U was pressed; the saved config catches up
    auto = []
    now = time.time() if now is None else now                # one clock for every draw of this frame
    asks = view["asks"] = open_asks(data, view.get("sent", ()))   # the rows the keys pick from
    view["lane_asks"] = lane_asks(data)                       # what a listed decision stands for (see shadows)
    if view.get("dsel") not in {d["id"] for d in asks}: view["dsel"] = view["dopt"] = None   # answered or gone
    if view.get("dmode") and not asks: view["dmode"] = False
    deciding = bool(view.get("dmode"))
    decide = {"on": deciding, "sel": view.get("dsel"), "opt": view.get("dopt"), "gone": view.get("sent", ())}
    numbering = view.setdefault("numbering", {})
    def draw(dense):
        col = {k for k, v in folds.items() if v} | set(auto)
        if deciding: col.discard("decisions")
        return render(data, plain=plain, show_all=show_all and not view.get("hide_done"), frame=frame, dense=dense,
                      links=links, interactive=True, collapsed=col, hidden=view.get("hidden", ()), now=now,
                      decide=decide, numbering=numbering).split("\n")
    lines = draw(False)
    if len(lines) > avail:
        lines = draw(True)
    for k in AUTO_FOLD:
        if len(lines) <= avail: break
        if k in folds or deciding and k == "decisions": continue
        auto.append(k)
        lines = draw(True)
    view["auto"] = auto
    view["collapsed"] = {k for k, v in folds.items() if v} | set(auto)
    p = Paint(not plain, links)
    cols = shutil.get_terminal_size((140, 40)).columns
    if len(lines) > avail:
        body = avail - 1
        top, sel = view.get("scroll", 0), view.get("dsel") if deciding else None
        if sel and view.get("shown") != sel:      # a newly picked decision scrolls into the pane, with its options
            at = next((i for i, l in enumerate(lines) if re.match(r"│ ▸\s?\d+\. ", ANSI.sub("", l))), None)
            if at is not None and not top <= at < top + body - 1: top = max(at - 1, 0)
        view["shown"] = sel
        top = max(0, min(top, len(lines) - body))
        below = len(lines) - top - body
        hint, age = f" ↑↓ PgUp PgDn scroll · 1-{len(numbering) or len(FOLD_KEYS)} fold · {top} above · {below} below ", github_age(p, data, now)
        lines = lines[top:top + body] + [p("╰─", C["frame"]) + p(hint, BOLD, C["pend"]) +   # the footer's age stays on
                                         p("─" * max(cols - width(hint) - width(age) - 5, 2), C["frame"]) + age + p("─╯", C["frame"])]
        view["scroll"], view["page"] = top, max(body - 2, 1)
    else:
        view["scroll"], view["page"], view["shown"] = 0, 1, view.get("dsel") if deciding else None
    toast = view.get("toast")
    if toast and now < toast["until"]:
        msg = clip(f" {toast['text']} ", cols - 6)
        lines[-1] = p("╰─", C["frame"]) + p(msg, BOLD, C[toast["color"]]) + p("─" * max(cols - width(msg) - 4, 2) + "╯", C["frame"])
    if fun:
        lines += [""] * max(avail - len(lines), 0)
        lines += nyan(p, cols, frame)
    # No row may be wider than the pane: a wrapped row makes the frame taller than the pane and it scrolls (shakes).
    lines = [fit_line(l, cols) for l in lines]
    view["screen"] = lines                    # what is on screen, so a click can find the link under it
    return lines


OSC8 = re.compile(r"\x1b\]8;;([^\x1b]*)\x1b\\")
CSI = re.compile(r"\x1b\[[0-9;?<]*[A-Za-z~]")              # a whole control sequence: arrows, Tab back, Delete (~)
SS3 = re.compile(r"\x1bO.", re.S)                         # an arrow or F1-F4 in application mode
MOUSE = re.compile(r"\x1b\[<(\d+);(\d+);(\d+)([Mm])")


def link_spans(line):
    """[(first column, end column, url)] for each OSC 8 link in a drawn line (columns start at 0)."""
    out, col, i, cur = [], 0, 0, None
    while i < len(line):
        m = OSC8.match(line, i)
        if m:
            if m.group(1): cur = (col, m.group(1))
            elif cur: out.append((cur[0], col, cur[1])); cur = None
            i = m.end(); continue
        m = CSI.match(line, i)
        if m: i = m.end(); continue
        col += width(line[i]); i += 1
    return out


def sync_hidden(view, data):
    """Keep a click's removal on screen until the saved config shows it: drop local removals the data now holds,
    and end a U (bring back all) once the data's list is empty."""
    saved = set(data.get("hidden") or ())
    view["hidden"] = set(view.get("hidden", ())) - saved
    if view.get("unhide_all") and not saved:
        view["unhide_all"] = False


def click(view, x, y):
    """A left click at terminal cell (x, y), both from 1: open the link under it. fleetview:fold:<section> folds or
    unfolds a section, fleetview:hide:<agent> removes a done lane from the view, fleetview:ask:<id> picks a decision,
    fleetview:answer:<id>:<n> picks its option n and a second click on the picked option sends it, and any other link
    opens in the browser."""
    screen = view.get("screen") or []
    if not 1 <= y <= len(screen): return None
    for a, b, url in link_spans(screen[y - 1]):
        if a <= x - 1 < b:
            if url.startswith("fleetview:fold:"):
                k = url.split(":", 2)[2]
                view.setdefault("folds", {})[k] = k not in view.get("collapsed", set())
            elif url.startswith("fleetview:hide:"):
                agent = url.split(":", 2)[2]
                view.setdefault("hidden", set()).add(agent)
                if view.get("on_hide"): view["on_hide"](agent)
            elif url.startswith(("fleetview:ask:", "fleetview:answer:")):
                parts = url.split(":")
                ask = next((d for d in live_asks(view) if d["id"] == parts[2]), None)
                opt = int(parts[3]) if len(parts) > 3 and parts[3].isdigit() else None
                if ask and opt is not None and view.get("dmode") and view.get("dsel") == ask["id"] and view.get("dopt") == opt:
                    decide(view, "send")
                elif ask:
                    view["dmode"] = True
                    pick(view, ask)
                    if opt is not None: view["dopt"] = opt
            elif view.get("on_open"):
                view["on_open"](url)
            return url
    return None


def toast(view, text, color="ok", seconds=4):
    """Show one line in place of the view's last row for a few seconds."""
    view["toast"] = {"text": text, "until": time.time() + seconds, "color": color}


def live_asks(view):
    """The rows fit() drew last, without the ones sent since then: keys and clicks in the same frame never pick a
    sent decision again."""
    sent = view.get("sent") or set()
    return [d for d in view.get("asks") or [] if d["id"] not in sent]


def pick(view, ask):
    """Pick a decision in the decision mode: its default option is picked with it (see default_option)."""
    view["dsel"], view["dopt"] = ask["id"], default_option(ask)


def decide(view, key):
    """One key of the decision mode, on the rows fit() drew last: up and down pick the decision above or below, 1-9
    the decision with that number, left and right (Tab) or a letter its option, and send answers the picked decision
    with the picked option. Sending hides the row at once and calls view["on_answer"](ask, option)."""
    drawn, asks = view.get("asks") or [], live_asks(view)  # a number is the one on screen; a sent row never comes back
    ids = [d["id"] for d in asks]
    cur = ids.index(view["dsel"]) if view.get("dsel") in ids else None
    ask, opt = (asks[cur] if cur is not None else None), view.get("dopt")
    if key in ("up", "down"):
        step = 1 if key == "down" else -1
        if asks: pick(view, asks[(cur + step) % len(asks)] if cur is not None else asks[0 if step > 0 else -1])
    elif key.isdigit():
        if 1 <= int(key) <= len(drawn) and drawn[int(key) - 1]["id"] in ids: pick(view, drawn[int(key) - 1])
    elif ask is None:
        if key == "send": toast(view, "pick a decision first: its number, or ↑ ↓", "pend", 3)
    elif key in ("left", "right"):
        n = len(ask["options"])
        if n: view["dopt"] = (0 if key == "right" else n - 1) if opt is None else (opt + (1 if key == "right" else -1)) % n
    elif key == "send":
        if opt is None or opt >= len(ask["options"]):
            toast(view, "pick an option first: a letter, or ← →", "pend", 3)
        elif ask["kind"] != "choice" and not ask["links"][opt]:
            toast(view, "nothing to open: the decision names no link", "pend", 3)
        else:                                  # its lane's own line closes with it: the lane still shows it a while
            also = [a["id"] for a in shadows(ask, view.get("lane_asks") or [])] if ask["source"] != "lane" else []
            view.setdefault("sent", set()).update([ask["id"]] + also)
            view["dsel"] = view["dopt"] = None    # the next Enter must not answer the next decision
            if view.get("on_answer"): view["on_answer"](dict(ask, also=also), opt)
    elif 0 <= ord(key.lower()) - 97 < len(ask["options"]):
        view["dopt"] = ord(key.lower()) - 97


SCROLL_KEYS = {"\x1b[A": -1, "\x1bOA": -1, "k": -1, "\x1b[B": 1, "\x1bOB": 1, "j": 1,
               "\x1b[5~": "-page", "\x1b[6~": "page", " ": "page", "\x1b[H": "home", "\x1bOH": "home", "g": "home",
               "\x1b[F": "end", "\x1bOF": "end", "G": "end"}


def split_partial(buf):
    """(complete input, unfinished escape sequence at the end). A read can stop in the middle of a mouse or arrow
    sequence; the rest arrives with the next read, so the tail waits instead of being read as separate keys."""
    j = buf.rfind("\x1b")
    if j == -1: return buf, ""
    tail = buf[j:]
    if tail[1:2] not in ("", "[", "O"): return buf, ""   # Esc and then a key (Esc q): no sequence to wait for
    if CSI.match(tail) or SS3.match(tail): return buf, ""    # a whole sequence: a mouse report, an arrow, Delete
    if len(tail) > 32: return buf, ""         # not a sequence we know: let apply_keys skip it
    return buf[:j], tail


DECIDE_KEYS = {"\x1b[A": "up", "\x1bOA": "up", "\x1b[B": "down", "\x1bOB": "down", "\x1b[D": "left", "\x1bOD": "left",
               "\x1b[C": "right", "\x1bOC": "right", "\t": "right", "\x1b[Z": "left", "\r": "send", "\n": "send"}


def apply_keys(view, chunk):
    """Apply typed keys to the view: arrows, PgUp/PgDn, Home/End, j/k/g/G scroll; 1-8 fold or unfold a section;
    0 clears the user's folds; x hides or shows every done lane; U brings back lanes removed with ✕; d answers the
    decisions (see decide: in that mode digits pick decisions, letters pick options, Enter sends and Esc leaves); q
    quits. Mouse: the wheel scrolls; a click opens the link under it (see click)."""
    folds = view.setdefault("folds", {})
    i = 0
    while i < len(chunk):
        m = MOUSE.match(chunk, i)
        if m:                                  # SGR mouse: 0 left press, 64/65 wheel up/down
            b, x, y = int(m.group(1)), int(m.group(2)), int(m.group(3))
            if b == 64: view["scroll"] = max(0, view.get("scroll", 0) - 3)
            elif b == 65: view["scroll"] = view.get("scroll", 0) + 3
            elif b == 0 and m.group(4) == "M": click(view, x, y)
            i = m.end()
            continue
        if view.get("dmode"):                  # the decision mode takes digits, letters, arrows, Enter and Esc
            seq, ch = next((k for k in sorted(DECIDE_KEYS, key=len, reverse=True) if chunk.startswith(k, i)), None), chunk[i]
            if seq:
                decide(view, DECIDE_KEYS[seq])
                i += len(seq)
                continue
            if ch == "\x1b" and chunk[i + 1:i + 2] not in ("[", "O"):   # Esc alone, not the start of a sequence
                view["dmode"] = False
                i += 1
                continue
            if ch in "0123456789" or ch.isascii() and ch.isalpha():
                decide(view, ch)
                i += 1
                continue
        seq = next((k for k in sorted(SCROLL_KEYS, key=len, reverse=True) if chunk.startswith(k, i)), None)
        if seq:
            step, page = SCROLL_KEYS[seq], view.get("page", 10)
            cur = view.get("scroll", 0)
            jumps = {"page": cur + page, "-page": cur - page, "home": 0, "end": 10 ** 6}
            view["scroll"] = max(0, jumps[step] if isinstance(step, str) else cur + step)
            i += len(seq)
            continue
        ch = chunk[i]; i += 1
        if ch == "\x1b":                       # an escape sequence we don't use: skip all of it
            m = CSI.match(chunk, i - 1) or SS3.match(chunk, i - 1)
            if m: i = m.end()
            continue
        num_map = view.get("numbering") or FOLD_KEYS
        if ch in num_map:
            k = num_map[ch]
            folds[k] = k not in view.get("collapsed", set())
        elif ch == "0":
            folds.clear()
        elif ch == "d":
            asks = live_asks(view)
            if asks:
                view["dmode"] = True
                if view.get("dsel") not in {d["id"] for d in asks}: pick(view, asks[0])
            else:
                toast(view, "✓ nothing waiting on you", "ok", 3)
        elif ch == "x":
            view["hide_done"] = not view.get("hide_done")
        elif ch == "U":
            view["hidden"], view["unhide_all"] = set(), True
            if view.get("on_unhide"): view["on_unhide"]()
        elif ch in "qQ":
            view["quit"] = True
    return view


def hide_list(hidden, add=None, remove=None):
    """The config's removed-lane list after adding `add` and removing `remove` ('all' clears it)."""
    out = [h for h in hidden if remove not in (h, "all")]
    if add and add not in out: out.append(add)
    return out


def save_config(path, change):
    """Read the config, apply change(cfg) and write it back; a running view refreshes when the file changes."""
    cfg = json.load(open(path)) if os.path.exists(path) else {}
    change(cfg)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    json.dump(cfg, open(path, "w"), indent=2)


LOG_FILE = os.path.expanduser("~/.fleet/view.log")


def log_error(where, err):
    """Append an error to ~/.fleet/view.log; the dashboard keeps running."""
    try:
        os.makedirs(os.path.dirname(LOG_FILE), exist_ok=True)
        with open(LOG_FILE, "a") as f:
            f.write(f"{time.strftime('%Y-%m-%d %H:%M:%S')} {where}: {type(err).__name__}: {err}\n")
    except OSError:
        pass


def discover(cfg):
    """Fill every config gap from the environment, so the view works for any user with no config file."""
    cfg = dict(cfg)
    if not cfg.get("github_author"):   # GitHub reads: in --watch they come from the cache (github), like every other
        cfg["github_author"] = github("login", None, lambda: run(["gh", "api", "user", "--jq", ".login"], timeout=30).strip() or None) or "@me"
    if not cfg.get("repos"):
        here = github("repo", os.getcwd(), lambda: run(["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
                                                       timeout=30).strip() or None)
        cfg["repos"] = [here] if here else []
    co = dict(cfg.get("coordinator") or {})
    co.setdefault("name", os.environ.get("USER", "operator"))
    co.setdefault("machine", (platform.node().split(".")[0]) + " (local)")
    co.setdefault("engagement", "fleet")
    cfg["coordinator"] = co
    # Configured orchestrators come first, in their order and with their labels; an engagement running on a machine
    # that the config doesn't name still shows, after them, so a lane started after the config was written is never lost.
    cfg["orchestrators"] = merge_orchestrators(cfg.get("orchestrators") or [], discovered_orchestrators())
    return cfg


def discovered_orchestrators():
    """One orchestrator per engagement with a named agent, on this Mac and every enabled Herdr machine.
    A named agent with no dash in its name is its own engagement (a single `fable` session, not a lane); an
    unnamed pane is not one. An engagement whose agent names exactly match one already found on another
    machine is the same fleet seen twice, and is skipped; one with different agents is kept."""
    machines = [("local", None)] if os.environ.get("HERDR_ENV") or shutil.which("herdr") else []
    for line in run(["herdr", "machine", "list"], timeout=30).splitlines():
        f = line.split("\t")
        if len(f) >= 5 and f[4].strip() == "enabled":
            machines.append((f[1].strip(), f[2].strip()))
    with cf.ThreadPoolExecutor(max_workers=8) as ex:
        lists = {i: ex.submit(agents_on, m[0]) for i, m in enumerate(machines)}
        lists = {i: f.result() or [] for i, f in lists.items()}
    orchs = []
    seen = set()
    for i, (m, ssh) in enumerate(machines):
        engs = {}
        for a in lists.get(i, []):
            name = a.get("name") or ""
            if not name:
                continue
            e = name.rsplit("-", 1)[0] if "-" in name else name
            engs.setdefault(e, set()).add(name)
        for e, names in engs.items():
            if (e, frozenset(names)) in seen:
                continue
            seen.add((e, frozenset(names)))
            orchs.append({"engagement": e, "label": e, "machine": m, "ssh": ssh})
    return orchs


def merge_orchestrators(configured, found):
    """The configured orchestrators, then each found one whose engagement isn't configured on that machine already."""
    have = {(o["engagement"], o["machine"]) for o in configured}
    return list(configured) + [o for o in found if (o["engagement"], o["machine"]) not in have]


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--plain", action="store_true", help="no colors (for Markdown reports)")
    ap.add_argument("--active", action="store_true", help="hide lanes whose agent is done (idle at its prompt)")
    ap.add_argument("--all", action="store_true", help="show agents from every repo, not just this checkout's")
    ap.add_argument("--json", action="store_true", help="print the collected tree as JSON")
    ap.add_argument("--watch", type=float, default=0, help="redraw every N seconds")
    ap.add_argument("--dense", action="store_true", help="no spacer lines between groups")
    ap.add_argument("--no-links", action="store_true", help="no OSC 8 hyperlinks on PR numbers")
    ap.add_argument("--nyan", action="store_true", help="Nyan Cat flies along the bottom (just for fun)")
    ap.add_argument("--set-goal", nargs=2, metavar=("AGENT", "GOAL"), help="record a lane's goal (agent name, e.g. rel-c-ui)")
    ap.add_argument("--pin", nargs=2, metavar=("AGENT", "OWNER/REPO#N"), help="map a lane to its PR")
    ap.add_argument("--refresh", action="store_true", help="tell a running --watch view to refresh now")
    ap.add_argument("--hide", metavar="AGENT", help="remove a done lane from the view (it comes back when it works again)")
    ap.add_argument("--unhide", metavar="AGENT", help="bring back a removed lane; 'all' brings back every one")
    ap.add_argument("--coordinator-pane", metavar="PANE", help="Herdr pane of the coordinator session (its latest message shows under Coordinator)")
    ap.add_argument("--config", default=DEFAULT_CONFIG)
    a = ap.parse_args()
    try: sys.stdout.reconfigure(encoding="utf-8", errors="replace")   # Windows consoles default to a legacy code page
    except Exception: pass
    if a.set_goal or a.pin or a.refresh or a.hide or a.unhide:
        cfg = {}
        if os.path.exists(a.config):
            cfg = json.load(open(a.config))
        if a.set_goal: cfg.setdefault("goals", {})[a.set_goal[0]] = a.set_goal[1]
        if a.pin: cfg.setdefault("pins", {})[a.pin[0]] = a.pin[1]
        if a.hide or a.unhide: cfg["hidden"] = hide_list(cfg.get("hidden", []), a.hide, a.unhide)
        if a.set_goal or a.pin or a.hide or a.unhide:
            os.makedirs(os.path.dirname(a.config), exist_ok=True)
            json.dump(cfg, open(a.config, "w"), indent=2)
        if a.refresh:   # a config change alone refreshes a --watch view within a second, and a new pin reads its PR
            os.makedirs(os.path.dirname(REFRESH_FILE), exist_ok=True)
            open(REFRESH_FILE, "a").close(); os.utime(REFRESH_FILE, None)
        return
    try:
        cfg = json.load(open(a.config))
    except FileNotFoundError:
        cfg = {}
    if a.coordinator_pane:
        cfg.setdefault("coordinator", {})["pane"] = a.coordinator_pane
    if a.watch: watch_github(cfg)
    cfg = discover(cfg)
    plain = a.plain or not sys.stdout.isatty() and not os.environ.get("FORCE_COLOR")
    if not a.watch:
        data = collect(cfg, all_repos=a.all)
        print(json.dumps(data, indent=2) if a.json else render(data, plain=plain, show_all=not a.active, dense=a.dense, links=not a.no_links, fun=a.nyan))
        return
    state = {"data": collect(cfg, all_repos=a.all), "cfg": cfg}
    def stamp():
        out = []
        for f in (a.config, REFRESH_FILE):
            try: out.append(os.path.getmtime(f))
            except OSError: out.append(0)
        return out
    def coord_pane():
        return a.coordinator_pane or (state["data"].get("coordinator") or {}).get("pane")
    def follow_coordinator():
        while True:                            # keep the coordinator's dialog (and with pulse its newest message) current
            time.sleep(3)
            pane = coord_pane()
            if pane:
                try:   # the config of the last refresh, and the fields go onto the data as it is after the reads
                    fields = coordinator_follow(pane, state["cfg"].get("pulse", True))
                    state["data"] = dict(state["data"], **fields)
                except Exception as e: log_error("coordinator", e)
    def refresh():
        last, seen, tick = time.time(), stamp(), 0
        cs = coordinator_state(coord_pane())
        while True:
            time.sleep(1)
            tick += 1
            try:   # the CRONS files and the last scan are re-read every second (local reads only), so a heartbeat
                # restarts its bar at once and a new scan's review gaps show at once. They go onto the data as it is
                # after the reads, so a follower step in between stays.
                crons, gaps = cron_jobs(state["cfg"], state["data"].get("cron_marks")), scan_gaps(state["cfg"])
                state["data"] = dict(state["data"], crons=crons, gaps=gaps)
            except Exception as e:
                log_error("crons", e)
            now = stamp()
            ncs = coordinator_state(coord_pane()) if tick % 2 == 0 else cs
            if now == seen and ncs == cs and time.time() - last < a.watch:
                continue                       # refresh early on a config, trigger-file or coordinator turn change
            forced = now[1] != seen[1]          # REFRESH_FILE was touched: this refresh reads GitHub again
            seen, cs, last = now, ncs, time.time()
            try:
                fresh = json.load(open(a.config)) if os.path.exists(a.config) else {}
                if a.coordinator_pane:
                    fresh.setdefault("coordinator", {})["pane"] = a.coordinator_pane
                watch_github(fresh)
                if forced or FORCED["due"]: force_github()
                fresh = discover(fresh)
                data = collect(fresh, prev=state["data"].get("reviewers") or [], all_repos=a.all)
                keep_since(state["data"].get("reviewers"), data["reviewers"])
                state["data"], state["cfg"] = data, fresh
            except Exception as e:  # keep the last good frame
                log_error("refresh", e)
    threading.Thread(target=refresh, daemon=True).start()
    threading.Thread(target=follow_coordinator, daemon=True).start()   # with pulse off it follows only an open dialog
    opener = "open" if sys.platform == "darwin" else "xdg-open"
    def deliver(ask, opt):
        """Send an answer picked in the view (see send_answer); its toast says how it went."""
        try:
            answer, problem = send_answer(ask, opt, state["cfg"], coord_pane(), opener=view["on_open"])
        except AlreadyAnswered:                # its row stays closed: the answer that counts is the earlier one
            toast(view, "answered already elsewhere", "muted", 6)
            return
        except Exception as e:                 # nothing was sent: the rows come back to answer again
            view["sent"].difference_update([ask["id"]] + list(ask.get("also") or []))
            toast(view, f"✗ not sent: {e}", "fail", 8)
            log_error("answer", e)
            return
        if problem: toast(view, f"! saved: {answer} · {problem}", "pend", 8)
        else: toast(view, f"✓ sent: {answer}", "ok", 4)
    courier = Courier(deliver)
    def on_answer(ask, opt):
        toast(view, ("… sending: " if ask["kind"] == "choice" else "… ") + ask["options"][opt], "muted", 30)
        courier.put(ask, opt)
    view = {"folds": {}, "scroll": 0, "hidden": set(), "sent": set(), "on_answer": on_answer,
            "on_hide": lambda ag: save_config(a.config, lambda c: c.__setitem__("hidden", hide_list(c.get("hidden", []), add=ag))),
            "on_unhide": lambda: save_config(a.config, lambda c: c.__setitem__("hidden", [])),
            "on_open": lambda url: subprocess.Popen([opener, url], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)}
    tty_state = None
    if sys.stdin.isatty():                     # keys: scroll, fold and answer (see apply_keys)
        try:
            import termios, tty, select
        except ImportError:                    # Windows has no termios: the view draws without key input
            termios = None
        if termios is not None:
            fd = sys.stdin.fileno()
            tty_state = termios.tcgetattr(fd)
            tty.setcbreak(fd)
            def read_keys():
                pending = ""
                while True:
                    if select.select([fd], [], [], 0.1 if pending else 0.5)[0]:
                        data = os.read(fd, 1024)
                        if not data: return            # the terminal went away
                        ready, pending = split_partial(pending + data.decode(errors="ignore"))
                    elif pending == "\x1b":            # Esc alone: no sequence followed it
                        ready, pending = pending, ""
                    else:
                        continue
                    try:
                        apply_keys(view, ready)
                    except Exception as e:         # a bad key never stops the view
                        log_error("keys", e)
            threading.Thread(target=read_keys, daemon=True).start()
    # alternate screen, hide cursor, mouse clicks and wheel as SGR events (the view opens links itself)
    mouse = "\x1b[?1000h\x1b[?1006h" if tty_state is not None else ""
    sys.stdout.write("\x1b[?1049h\x1b[?25l" + mouse)
    try:
        frame = 0
        last_data = None
        last_out = None
        while not view.get("quit"):
            if state["data"] is not last_data:     # clicks the saved config now holds need no local copy
                last_data = state["data"]
                sync_hidden(view, last_data)
            rows = shutil.get_terminal_size((140, 40)).lines
            out = fit(state["data"], rows, view, frame=frame, show_all=not a.active, links=not a.no_links, fun=a.nyan, plain=plain)
            if out != last_out:                    # repainting an unchanged frame every tick flickers on some terminals
                last_out = out
                sys.stdout.write("\x1b[H" + "\n".join(l + "\x1b[K" for l in out) + "\x1b[J")
                sys.stdout.flush()
            frame += 1
            time.sleep(0.12)
    except KeyboardInterrupt:
        log_error("main", KeyboardInterrupt("Ctrl-C"))
    except Exception as e:
        log_error("main", e)
        raise
    finally:
        try:
            if not courier.drain(20): log_error("answer", TimeoutError("an answer was not sent before the view quit"))
        except KeyboardInterrupt:
            pass
        if view.get("quit"): log_error("main", SystemExit("q pressed"))
        sys.stdout.write(("\x1b[?1006l\x1b[?1000l" if mouse else "") + "\x1b[?25h\x1b[?1049l"); sys.stdout.flush()
        if tty_state is not None:
            termios.tcsetattr(sys.stdin.fileno(), termios.TCSADRAIN, tty_state)

if __name__ == "__main__":
    main()
