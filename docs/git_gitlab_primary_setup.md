# Git topology: GitLab on limbic is primary, GitHub is a slow downstream mirror

Status as of 2026-10-22: **everything below has been carried out.** The credential (§2), the
limbic and pineal remotes (§3, §4) and the daily sync job (§5) are all in place and verified.
The command blocks are kept as a record of what was run and as a recipe for rebuilding.

## 1. Target topology

```
  limbic dev tree  ─┐
                    ├──►  GitLab on limbic (primary, "origin")  ──daily job──►  GitHub ("github")
  pineal dev tree  ─┘     http://limbic:8929  /  ssh :2224
```

- **Day-to-day work** pushes to and pulls from `origin`, which is the GitLab on limbic, from
  both hosts.
- **GitHub** is only ever pushed to by the scheduled sync job on limbic. Nobody pushes there by
  hand any more, and nothing pulls from it.
- The sync is **one-way** (GitLab to GitHub), **non-forced** and **non-pruning**. A diverged
  branch on GitHub is reported as a failure and left alone. A branch deleted on GitLab stays
  on GitHub until someone removes it there.

Repos covered (all five sync, per the 2026-10-21 decision):

| GitLab project | GitHub repo |
|---|---|
| `root/clara-cerebellum` | `stancampbell3/clara-cerebellum` |
| `root/lildaemon` | `stancampbell3/lildaemon` |
| `root/dagda` | `stancampbell3/dagda` |
| `root/ochecho` | `stancampbell3/ochecho` |
| `root/seashell` | `stancampbell3/seashell_website` (note the different name) |

`SilentHunter` on pineal is a third-party fork. It is not ours and is not part of this.

## 2. One-time credential on limbic: Done (2026-10-22)

Limbic talks to GitLab over HTTP. The repos' old `origin` URLs embedded a personal access
token in plain text inside `.git/config` (and that directory is on the shared NFS mount).
Three of those tokens were expired, which is why dagda, ochecho and seashell failed to fetch.

What was actually done differs from the first plan (which reused `clara_dev_token`): a
dedicated, narrower token was minted instead, so the git credential no longer depends on the
dev token and lasts until October 2027.

- A root personal access token `limbic-git-credential-<timestamp>` with scopes
  `read_repository` and `write_repository`, expiring **2027-10-22**, was created with
  `gitlab-rails runner` inside the `gitlab` container and written to `~/.git-credentials`
  (mode 600) without ever being printed.
- `git config --global credential.http://limbic:8929.helper store`. The helper is scoped to
  the GitLab URL on purpose. A plain global `credential.helper store` would also offer GitHub
  credentials to the `store` helper and could write them to `~/.git-credentials`.
- Check: `git ls-remote http://limbic:8929/root/dagda.git master` succeeds with no token in
  the URL.

Gotcha when minting the token: `gitlab-rails runner "..."` is sensitive to line breaks. If a
long command is pasted and wraps after `puts`, Ruby evaluates a bare `puts` (blank line) and
discards the token on the next line, so the capture is empty. Keep the Ruby on one line, or
put the command in a script file.

GitHub needs nothing new: limbic already authenticates through `gh auth git-credential`
(`credential.https://github.com.helper` in `~/.gitconfig`).

## 3. Limbic remotes (`/mnt/moonpool/Development`)

### Done: clara-cerebellum and lildaemon

- Every GitHub branch and every local-only branch was pushed to GitLab, one branch at a time.
  Pushing many refs in a single command was declined by GitLab's pre-receive hook, so the
  per-branch loop is what works.
- GitLab and GitHub now have identical tips for every GitHub branch.
- Local branches that exist on GitLab now track `origin/<branch>`, and `master` tracks
  `origin/master`.

### Done (2026-10-22): all five repos

Verified after the fact: `grep -l glpat */.git/config` finds nothing, all five `origin` URLs
are the clean form, `master` tracks `origin/master`, and GitLab and GitHub `master` are
identical (0 commits either way) for every repo.

Recipe (for reference):

Replace the token-bearing URL with a clean one:

```bash
cd /mnt/moonpool/Development
for r in clara-cerebellum dagda lildaemon ochecho seashell; do
  git -C $r remote set-url origin http://limbic:8929/root/$r.git
done
grep -l glpat */.git/config        # expect no output
```

For dagda, ochecho and seashell, bring GitLab up to date and repoint `master`:

```bash
for r in dagda ochecho seashell; do
  git -C $r fetch origin && git -C $r fetch github
  git -C $r rev-list --left-right --count origin/master...github/master   # GitLab-only, GitHub-only
  git -C $r push origin master
  git -C $r branch -u origin/master master
done
```

If GitHub turns out to have commits GitLab lacks, push them first
(`git push origin refs/remotes/github/<branch>:refs/heads/<branch>`), one branch at a time.

## 4. Pineal remotes (`~/widebody/Development`): Done (2026-10-22)

Done over SSH from limbic. Pineal's clara-cerebellum and lildaemon were already in sync with
GitLab (their "ahead" counts were stale refs). dagda, ochecho and seashell now have `origin` =
GitLab and `github` = the old GitHub remote; none had commits GitLab lacked, and all five
`master` branches track `origin/master`. Working trees were not pulled: dagda and seashell on
pineal are 3 commits behind GitLab until a `git pull --ff-only`.

Recipe (for reference):

Pineal's two-remote repos (clara-cerebellum, lildaemon) are already right: `origin` is
`git@gitlab-limbic:root/<repo>.git` and `github` is GitHub. Only their `master` upstream needs
checking:

```bash
cd ~/widebody/Development
for r in clara-cerebellum lildaemon; do
  git -C $r fetch origin && git -C $r branch -u origin/master master
done
```

The other three have `origin` pointing at GitHub. Rename it and add GitLab. The `gitlab-limbic`
alias (`~/.ssh/config`: host `limbic`, port 2224, user `git`, key `gitlab_limbic_ed25519`)
already authenticates, so no new credential is needed:

```bash
cd ~/widebody/Development
for r in dagda ochecho seashell; do
  git -C $r remote rename origin github
  git -C $r remote add origin git@gitlab-limbic:root/$r.git
  git -C $r fetch origin
  git -C $r rev-list --left-right --count master...origin/master   # check before repointing
  git -C $r branch -u origin/master master
done
```

If pineal's `master` has commits GitLab lacks, push them (`git push origin master`) before
repointing. Do not use the NFS copy at `/mnt/moonpool` for git work from pineal; use the
`~/widebody` clones.

After this, deploys on pineal pull from `origin` (GitLab), so a push to GitLab reaches pineal
immediately. The old habit of also pushing to `github` by hand is no longer needed.

## 5. The daily GitLab-to-GitHub sync job (limbic): Done (2026-10-22)

Installed on limbic; the first manual run pushed `ok` for all five repos. The timer fires
daily at about 03:30 UTC (randomized by up to 10 minutes) and linger is enabled. The files
below are exactly what is installed.

Runs as `stanc` on limbic as a systemd user timer. It keeps its own bare mirror clones, so it
never touches the working trees.

### 5.1 Repo list

`~/.config/gitlab-github-sync/repos.conf`, one repo per line (`<gitlab name> <github url>`).
**To stop syncing a repo, delete or comment out its line.**

```
clara-cerebellum  https://github.com/stancampbell3/clara-cerebellum.git
lildaemon         https://github.com/stancampbell3/lildaemon.git
dagda             https://github.com/stancampbell3/dagda.git
ochecho           https://github.com/stancampbell3/ochecho.git
seashell          https://github.com/stancampbell3/seashell_website.git
```

### 5.2 Script

`~/.local/bin/sync-gitlab-to-github` (mode 755):

```bash
#!/usr/bin/env bash
# Push GitLab branches and tags to GitHub. One-way, no force, no prune.
set -uo pipefail
CONF=${SYNC_CONF:-$HOME/.config/gitlab-github-sync/repos.conf}
DIR=${SYNC_DIR:-$HOME/.local/share/gitlab-github-sync}
mkdir -p "$DIR"
rc=0
while read -r name github_url; do
  [[ -z $name || $name == \#* ]] && continue
  gd=$DIR/$name.git
  if [[ ! -d $gd ]]; then
    git clone --mirror -q "http://limbic:8929/root/$name.git" "$gd" \
      || { echo "FAIL clone $name"; rc=1; continue; }
  fi
  git -C "$gd" fetch -q --prune origin || { echo "FAIL fetch $name"; rc=1; continue; }
  if git -C "$gd" push -q "$github_url" 'refs/heads/*:refs/heads/*' 'refs/tags/*:refs/tags/*'; then
    echo "ok   $name"
  else
    echo "FAIL push $name (diverged on GitHub, or auth)"; rc=1
  fi
done < "$CONF"
exit $rc
```

### 5.3 Units

`~/.config/systemd/user/gitlab-github-sync.service`:

```ini
[Unit]
Description=Push GitLab (limbic) branches and tags to GitHub

[Service]
Type=oneshot
ExecStart=%h/.local/bin/sync-gitlab-to-github
```

`~/.config/systemd/user/gitlab-github-sync.timer`:

```ini
[Unit]
Description=Daily GitLab to GitHub sync

[Timer]
OnCalendar=*-*-* 03:30:00
Persistent=true
RandomizedDelaySec=10m

[Install]
WantedBy=timers.target
```

### 5.4 Install and check

```bash
systemctl --user daemon-reload
systemctl --user enable --now gitlab-github-sync.timer
loginctl enable-linger stanc            # so it runs while nobody is logged in
systemctl --user start gitlab-github-sync.service     # first run, by hand
journalctl --user -u gitlab-github-sync.service -n 20
systemctl --user list-timers gitlab-github-sync.timer
```

The first run clones all five repos and pushes anything GitHub lacks. A repo reported as
`FAIL push` has diverged on GitHub. Look at it by hand; the job never forces.

## 6. Everyday workflow afterwards

- Commit, then `git push` (to GitLab). Pull with `git pull`. Both hosts.
- To confirm the mirror caught up: `git ls-remote github refs/heads/master` against
  `git ls-remote origin refs/heads/master`, or read the journal output above.
- To publish to GitHub right now instead of waiting for 03:30:
  `systemctl --user start gitlab-github-sync.service`.
- If GitLab itself looks dead: it ran as a Docker container named `gitlab` and has exited
  unnoticed before (code 137, 2026-09-14). `docker ps --filter name=gitlab`, and
  `docker start gitlab` takes about two minutes to become healthy.

## 7. Caveats

- The git credential token expires **2027-10-22**. The credential in §2 and the sync job
  both stop working then; mint a replacement the same way before that date.
- `clara_dev_token` (expires 2027-01-01, `api` scope) still exists for other uses but is no
  longer the git credential. The new token is limited to `read_repository` and
  `write_repository`, but it still acts as `root`, so anyone who can read `~/.git-credentials`
  on limbic can read and write every repo. That is why the file is mode 600.
- `bootstrap-cli` and `clara-development` GitLab tokens have expired; anything still using
  either needs a new token.
- Commits made directly on GitHub (web edits, merged pull requests) are never pulled back by
  the job. Fetch them into GitLab by hand if that ever happens.
