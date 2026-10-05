#!/usr/bin/env python3
"""sentry.py — read OpenRig's Sentry issues and events from the terminal.

Usage:
  ./scripts/sentry.py issues [-q QUERY] [-p PERIOD]   unresolved issues (default 14d)
  ./scripts/sentry.py events OPENRIG-N [-n N] [-b N]  latest events: release, OS, device, breadcrumbs
  ./scripts/sentry.py resolve OPENRIG-N               mark an issue resolved

Token: SENTRY_AUTH_TOKEN, or `token=` under [auth] in ~/.sentryclirc
(personal token, scopes org:read project:read event:read; event:write for resolve).
"""
import argparse
import configparser
import json
import os
import sys
import urllib.parse
import urllib.request

ORG = os.environ.get("SENTRY_ORG", "joao-paulo-faria")
PROJECT = os.environ.get("SENTRY_PROJECT", "openrig")
API = "https://sentry.io/api/0"


def token():
    tok = os.environ.get("SENTRY_AUTH_TOKEN")
    if tok:
        return tok
    cfg = configparser.ConfigParser()
    cfg.read(os.path.expanduser("~/.sentryclirc"))
    tok = cfg.get("auth", "token", fallback="")
    if not tok:
        sys.exit("no Sentry token: set SENTRY_AUTH_TOKEN or [auth] token= in ~/.sentryclirc")
    return tok


def call(path, params=None, method="GET", body=None):
    url = f"{API}{path}"
    if params:
        url += "?" + urllib.parse.urlencode(params)
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {token()}")
    req.add_header("Content-Type", "application/json")
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)


def issue_id(short_id):
    found = call(f"/organizations/{ORG}/shortids/{short_id}/")
    return found["groupId"]


def cmd_issues(args):
    query = f"project:{PROJECT} {args.query}".strip()
    rows = call(f"/organizations/{ORG}/issues/", {"query": query, "statsPeriod": args.period})
    if not rows:
        print("no issues")
    for i in rows:
        print(f'{i["shortId"]:<12} {i["count"]:>5}x  last {i["lastSeen"][:16]}  first {i["firstSeen"][:16]}  {i["title"]}')


def cmd_events(args):
    gid = issue_id(args.issue)
    events = call(f"/organizations/{ORG}/issues/{gid}/events/", {"full": "true"})[: args.n]
    for e in events:
        tags = {t["key"]: t["value"] for t in e.get("tags", [])}
        print(f'== {e["dateCreated"][:19]}  {tags.get("release", "?")}  {tags.get("os", "?")}  {tags.get("device", "?")}')
        print(f'   {e.get("message") or e.get("title")}')
        for entry in e["entries"]:
            if entry["type"] == "breadcrumbs" and args.b:
                for b in entry["data"]["values"][-args.b :]:
                    ts = (b.get("timestamp") or "")[11:19]
                    print(f'   {ts} {b.get("level", ""):<7} {(b.get("message") or "")[:200]}')
            if entry["type"] == "exception":
                for ex in entry["data"]["values"]:
                    print(f'   EXC {ex.get("type")}: {ex.get("value")}')
                    frames = (ex.get("stacktrace") or {}).get("frames") or []
                    for f in frames[-15:]:
                        print(f'     {f.get("function")}  {f.get("filename")}:{f.get("lineNo")}')


def cmd_resolve(args):
    call(f"/organizations/{ORG}/issues/{issue_id(args.issue)}/", method="PUT", body={"status": "resolved"})
    print(f"{args.issue} resolved")


def main():
    p = argparse.ArgumentParser(description="OpenRig Sentry reader")
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("issues")
    s.add_argument("-q", "--query", default="is:unresolved")
    s.add_argument("-p", "--period", default="14d")
    s.set_defaults(fn=cmd_issues)
    s = sub.add_parser("events")
    s.add_argument("issue")
    s.add_argument("-n", type=int, default=3, help="events to show")
    s.add_argument("-b", type=int, default=20, help="breadcrumbs per event (0 = none)")
    s.set_defaults(fn=cmd_events)
    s = sub.add_parser("resolve")
    s.add_argument("issue")
    s.set_defaults(fn=cmd_resolve)
    args = p.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
