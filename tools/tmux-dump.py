#!/usr/bin/env python3
"""tmux-Sessions › Windows › Panes › Claude-Sessions dumpen und in Kadrell importieren.

  tools/tmux-dump.py dump [-o dump.json]          Pane-Baum + Claude-Session je Pane als JSON
  tools/tmux-dump.py import dump.json [--dry-run] Sessions per `claude --bg --resume` als
                                                  Hintergrund-Sessions starten und je tmux-Session
                                                  eine Kadrell-Gruppe in groups.json anlegen

Zuordnung Pane → Claude: `claude agents --json --all` liefert die `pid` jeder Session, tmux die
`pane_pid` (die Shell). Der Prozessbaum aus `ps` verbindet beides. Nur stdlib, kein uv nötig.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import uuid
from collections import defaultdict
from datetime import datetime, timezone

PALETTE = ["#fab387", "#cba6f7", "#f5c2e7", "#89b4fa", "#a6e3a1",
           "#94e2d5", "#f9e2af", "#eba0ac", "#b4befe", "#74c7ec"]  # Theme.palette der App
GROUPS_JSON = os.path.expanduser("~/Library/Application Support/de.malura.kadrell/groups.json")
GENERIC_WINDOW_NAMES = {"zsh", "bash", "claude", "node"}


def run(cmd, cwd=None):
    return subprocess.run(cmd, capture_output=True, text=True, cwd=cwd)


def tmux_panes():
    fmt = "#{session_name}\t#{window_index}\t#{window_name}\t#{pane_index}\t#{pane_pid}\t#{pane_current_path}"
    r = run(["tmux", "list-panes", "-a", "-F", fmt])
    if r.returncode != 0:
        sys.exit(f"tmux: {r.stderr.strip() or 'kein Server'}")
    panes = []
    for line in r.stdout.splitlines():
        s, w, wn, p, pid, path = line.split("\t")
        panes.append({"session": s, "window": int(w), "windowName": wn, "pane": int(p),
                      "pid": int(pid), "cwd": path})
    return panes


def process_children():
    """pid → [Kind-PIDs]. `/bin/ps` direkt, weil Shell-Wrapper (rtk) die Liste kürzen."""
    kids = defaultdict(list)
    for line in run(["/bin/ps", "-axo", "pid=,ppid="]).stdout.splitlines():
        pid, ppid = line.split()
        kids[int(ppid)].append(int(pid))
    return kids


def descendants(pid, kids):
    out, stack = [], [pid]
    while stack:
        p = stack.pop()
        for c in kids.get(p, []):
            out.append(c)
            stack.append(c)
    return out


def claude_agents():
    r = run(["claude", "agents", "--json", "--all"])
    start = r.stdout.find("[")
    if r.returncode != 0 or start < 0:
        sys.exit(f"claude agents: {r.stderr.strip() or r.stdout.strip()}")
    return json.loads(r.stdout[start:])


def display_name(agent, pane):
    """Haiku-Titel behalten; den Zähler-Namen `<ordner>-NN` durch den tmux-Fensternamen ersetzen."""
    name = agent.get("name") or ""
    base = os.path.basename(agent.get("cwd", ""))
    if name and not re.fullmatch(re.escape(base) + r"-[0-9a-f]+", name):
        return name
    if pane["windowName"] not in GENERIC_WINDOW_NAMES:
        return f'{pane["session"]}/{pane["windowName"]}' + (f'.{pane["pane"]}' if pane.get("multi") else "")
    return name or f'{pane["session"]}:{pane["window"]}.{pane["pane"]}'


def dump():
    kids = process_children()
    by_pid = {a["pid"]: a for a in claude_agents() if a.get("pid")}
    groups, seen = {}, set()
    panes = tmux_panes()
    per_window = defaultdict(int)
    for pane in panes:
        per_window[(pane["session"], pane["window"])] += 1
    for pane in panes:
        pane["multi"] = per_window[(pane["session"], pane["window"])] > 1
        hits = [by_pid[p] for p in [pane["pid"]] + descendants(pane["pid"], kids) if p in by_pid]
        for agent in hits:
            if agent["sessionId"] in seen:  # dieselbe Session in zwei Panes (z. B. --resume nebenan)
                continue
            seen.add(agent["sessionId"])
            g = groups.setdefault(pane["session"], {"name": pane["session"], "cwd": None, "sessions": []})
            g["sessions"].append({
                "sessionId": agent["sessionId"], "name": display_name(agent, pane), "cwd": agent["cwd"],
                "kind": agent.get("kind"), "pid": agent["pid"], "status": agent.get("status"),
                "startedAt": agent.get("startedAt"),
                "tmux": {k: pane[k] for k in ("session", "window", "windowName", "pane")},
            })
    for g in groups.values():
        cwds = [s["cwd"] for s in g["sessions"]]
        g["cwd"] = max(set(cwds), key=cwds.count)
    return {"version": 1, "dumpedAt": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "groups": list(groups.values())}


def resume_background(session, dry_run):
    cmd = ["claude", "--bg", "--resume", session["sessionId"], "--name", session["name"]]
    if dry_run:
        print(f'  [dry-run] (cd {session["cwd"]} && {" ".join(cmd)})')
        return None
    r = run(cmd, cwd=session["cwd"])
    out = r.stdout + r.stderr
    m = re.search(r"backgrounded · ([0-9a-f]{8})", out)
    if r.returncode != 0 or not m:
        print(f'  FEHLER {session["name"]}: {out.strip()}', file=sys.stderr)
        return None
    note = " (Kopie, Original läuft weiter)" if "copy" in out else ""
    print(f'  {m.group(1)}  {session["name"]}{note}')
    return m.group(1)


def merge_groups(existing, name, cwd, short_ids):
    """Gruppe mit gleichem Namen erweitern, sonst neue anlegen. Gibt die neue Liste zurück."""
    groups = [dict(g) for g in existing]
    for g in groups:
        if g["name"] == name:
            g["sessionIds"] = g["sessionIds"] + [i for i in short_ids if i not in g["sessionIds"]]
            return groups
    used = {g["color"] for g in groups}
    color = next((c for c in PALETTE if c not in used), PALETTE[len(groups) % len(PALETTE)])
    groups.append({"id": str(uuid.uuid4()), "name": name, "color": color, "cwd": cwd, "sessionIds": list(short_ids)})
    return groups


def do_import(path, dry_run, groups_json):
    data = json.load(open(path))
    if data.get("version") != 1:
        sys.exit("unbekanntes Dump-Format")
    if groups_json == GROUPS_JSON and not dry_run and run(["pgrep", "-x", "Kadrell"]).returncode == 0:
        sys.exit("Kadrell läuft und würde groups.json überschreiben. Erst beenden.")
    groups = json.load(open(groups_json)) if os.path.exists(groups_json) else []
    for g in data["groups"]:
        print(f'{g["name"]}  ({len(g["sessions"])} Sessions)')
        ids = [i for i in (resume_background(s, dry_run) for s in g["sessions"]) if i]
        if ids:
            groups = merge_groups(groups, g["name"], g["cwd"], ids)
    if dry_run:
        return
    os.makedirs(os.path.dirname(groups_json), exist_ok=True)
    with open(groups_json, "w") as f:
        json.dump(groups, f, indent=2, sort_keys=True)
    print(f"{groups_json} geschrieben: {len(groups)} Gruppen")


def selftest():
    a = merge_groups([], "malura", "/x", ["aaaaaaaa"])
    assert len(a) == 1 and a[0]["color"] == PALETTE[0] and a[0]["sessionIds"] == ["aaaaaaaa"]
    b = merge_groups(a, "malura", "/x", ["aaaaaaaa", "bbbbbbbb"])
    assert b[0]["sessionIds"] == ["aaaaaaaa", "bbbbbbbb"] and len(b) == 1
    c = merge_groups(b, "homelab", "/y", ["cccccccc"])
    assert c[1]["color"] == PALETTE[1]
    pane = {"session": "malura", "window": 2, "windowName": "ideas", "pane": 1}
    assert display_name({"name": "malura-12", "cwd": "/p/malura"}, pane) == "malura/ideas"
    assert display_name({"name": "bloom-f5", "cwd": "/p/bloom"}, {**pane, "multi": True}) == "malura/ideas.1"
    assert display_name({"name": "haiku-title", "cwd": "/p/malura"}, pane) == "haiku-title"
    assert display_name({"name": "malura-12", "cwd": "/p/malura"}, {**pane, "windowName": "zsh"}) == "malura-12"
    print("selftest ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("dump")
    d.add_argument("-o", "--output")
    i = sub.add_parser("import")
    i.add_argument("dump")
    i.add_argument("--dry-run", action="store_true")
    i.add_argument("--groups-json", default=GROUPS_JSON)
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "dump":
        out = json.dumps(dump(), indent=2, ensure_ascii=False)
        if args.output:
            open(args.output, "w").write(out + "\n")
            n = sum(len(g["sessions"]) for g in json.loads(out)["groups"])
            print(f'{args.output}: {n} Claude-Sessions in {len(json.loads(out)["groups"])} tmux-Sessions')
        else:
            print(out)
    elif args.cmd == "import":
        do_import(args.dump, args.dry_run, args.groups_json)
    else:
        selftest()


if __name__ == "__main__":
    main()
