# Instructions for Claude Code

This is a public fork of Second-Boot/Open-Unleashed. The owner's own work (the
tvOS port, NFS Haven / GameHaven, nfs-haven-server) lives on the `tvos-port`
branch. Upstream's files follow upstream's own rules (CONTRIBUTING.md).

## Privacy (applies to every commit, by anyone, on any machine)

This repository is **public**. Nothing that identifies the owner's home, network
or computers may be committed:

- No real IP addresses: home LAN, Tailscale (100.x), or any computer's address.
  In docs and tests use examples: `192.168.1.x`, `10.0.0.x`, `100.64.x.x`, or
  RFC 5737 `192.0.2.x`.
- No real hostnames or domains: no tailnet names (`*.ts.net` -> write
  `tower.<your-tailnet>.ts.net`), no personal domains, no computer names.
  `tower.local` (Unraid's default name) is fine.
- No people's names in code or tests (use `Player`), no container names or
  server paths that are specific to the owner's setup.
- Commit author email must be the GitHub noreply address
  (`186242125+dannyelson82@users.noreply.github.com`), never a computer's local
  address such as `name@My-Mac.local`. Check with `git config user.email`.
- Real setup values go in the gitignored `LOCAL_NOTES.md`, never in tracked files.

`scripts/privacy-check.py` enforces this in CI (`.github/workflows/privacy.yml`)
on every push and pull request; run it before pushing:
`python3 scripts/privacy-check.py upstream/main HEAD --exclude upstream/main`.
A line that is genuinely an example can carry the comment `privacy-check: allow`.

## Other rules

- Work on a branch and open a pull request into `tvos-port`; Claude squash-merges
  once the checks pass. Signed commits, conventional commit messages.
- Never commit secrets, game files, disc images or save data.
