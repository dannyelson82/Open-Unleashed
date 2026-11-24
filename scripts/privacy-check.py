#!/usr/bin/env python3
"""Fail if new commits would make personal infrastructure details public.

Usage: privacy-check.py BASE [HEAD] [--exclude REF ...]
Checks each non-merge commit in BASE..HEAD (skipping commits reachable from any
--exclude REF, e.g. the upstream project's main) and the lines it adds.

Checks, using generic patterns only (the real values are never written here):
- author/committer emails that are a computer's local address (user@host.local,
  .lan, .home, .localdomain): use the GitHub noreply address instead;
- private IPv4 addresses (10/8, 172.16/12, 192.168/16) other than the example
  ones below, and Tailscale addresses (100.64.0.0/10) outside 100.64.x.x;
- Tailscale MagicDNS names (*.ts.net) unless they're placeholders.

Allowed examples: 192.168.1.x, 10.0.0.x, 172.17.0.x (Docker), 100.64.x.x,
RFC 5737 192.0.2.x / 198.51.100.x / 203.0.113.x, and ts.net names containing
"example" or a <placeholder>. A line containing "privacy-check: allow" is skipped.
"""

import ipaddress
import re
import subprocess
import sys

LOCAL_EMAIL = re.compile(r"@[^@\s]+\.(local|lan|home|localdomain|internal)$", re.I)
IPV4 = re.compile(r"(?<![\d.])(\d{1,3}(?:\.\d{1,3}){3})(?![\d.])")
TS_NET = re.compile(r"[A-Za-z0-9<>._-]+\.ts\.net\b")
ALLOWED_NETS = [ipaddress.ip_network(n) for n in (
    "192.168.1.0/24", "10.0.0.0/24", "172.17.0.0/24", "100.64.0.0/16",
)]
TAILSCALE = ipaddress.ip_network("100.64.0.0/10")
ALLOW_MARK = "privacy-check: allow"


def git(*args: str) -> str:
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def bad_ip(text: str) -> bool:
    try:
        ip = ipaddress.ip_address(text)
    except ValueError:
        return False
    if any(ip in n for n in ALLOWED_NETS):
        return False
    return ip.is_private and not ip.is_loopback and not ip.is_link_local or ip in TAILSCALE


def bad_ts_name(name: str) -> bool:
    return not ("<" in name or "example" in name.lower())


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    args = sys.argv[1:]
    exclude = [args[i + 1] for i, a in enumerate(args) if a == "--exclude" and i + 1 < len(args)]
    pos = [a for i, a in enumerate(args) if a != "--exclude" and (i == 0 or args[i - 1] != "--exclude")]
    base, head = pos[0], pos[1] if len(pos) > 1 else "HEAD"
    problems: list[str] = []

    log = git("log", "-p", "-U0", "--no-merges", "--no-color", "--no-ext-diff",
              "--format=@@commit %h %ae %ce", f"{base}..{head}", *[f"^{r}" for r in exclude])
    sha, path = "?", "?"
    for line in log.splitlines():
        if line.startswith("@@commit "):
            sha, *emails = line.split()[1:]
            for email in set(emails):
                if LOCAL_EMAIL.search(email):
                    problems.append(f"commit {sha}: email is a computer's local address "
                                    f"({email.split('@')[1]}); set git config user.email to "
                                    "your GitHub noreply address")
            continue
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else line[4:]
            continue
        if not line.startswith("+") or ALLOW_MARK in line:
            continue
        for ip in IPV4.findall(line):
            if bad_ip(ip):
                problems.append(f"{path} ({sha}): private or Tailscale address {ip} "
                                "(use 192.168.1.x / 100.64.x.x examples)")
        for name in TS_NET.findall(line):
            if bad_ts_name(name):
                problems.append(f"{path} ({sha}): Tailscale name {name} (use tower.<your-tailnet>.ts.net)")

    for p in dict.fromkeys(problems):
        print(f"::error::{p}")
    if problems:
        print(f"{len(set(problems))} privacy problem(s). See CLAUDE.md, 'Privacy'.")
        return 1
    print("privacy check: nothing personal found")
    return 0


if __name__ == "__main__":
    sys.exit(main())
