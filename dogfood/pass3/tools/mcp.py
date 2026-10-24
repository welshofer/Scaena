#!/usr/bin/env python3
"""A small stdio JSON-RPC client for `scaena mcp`, driven the way an agent drives it.

Each run starts the server (cwd = the dogfood3 folder, so bundles are named `decks/<deck>`),
initializes, then runs the calls given, logging every request and response to a JSONL
transcript per task (`transcripts/<task>.jsonl`, appended). Image content is saved to
`evidence/<task>/` and replaced in the transcript by its path.

Usage:
  mcp.py TASK list-tools [--save FILE]
  mcp.py TASK list-resources
  mcp.py TASK read URI [--save FILE]
  mcp.py TASK call TOOL JSON_ARGS [--name LABEL] [--quiet]
  mcp.py TASK calls FILE.json        # [[tool, args, label?], ...]
"""
import base64
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
S = os.path.dirname(ROOT)
SCAENA = os.environ.get("SCAENA", os.path.join(S, "dogfood2/target/release/scaena"))


class Server:
    def __init__(self, task):
        self.task = task
        self.transcript = os.path.join(ROOT, "transcripts", f"{task}.jsonl")
        self.evidence = os.path.join(ROOT, "evidence", task)
        os.makedirs(os.path.dirname(self.transcript), exist_ok=True)
        os.makedirs(self.evidence, exist_ok=True)
        env = dict(os.environ)
        env.setdefault("SCAENA_AUTHOR", "agent:dogfood3")
        self.p = subprocess.Popen(
            [SCAENA, "mcp"], cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, env=env, text=True, bufsize=1,
        )
        self.next_id = 1
        self.saved = []

    def lines(self):
        with open(self.transcript) as f:
            return sum(1 for _ in f)

    def log(self, entry):
        with open(self.transcript, "a") as f:
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
        return self.lines()

    def send(self, method, params=None, notify=False, label=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        if not notify:
            msg["id"] = self.next_id
            self.next_id += 1
        self.p.stdin.write(json.dumps(msg) + "\n")
        self.p.stdin.flush()
        line_req = self.log({"dir": "->", "t": time.time(), "label": label, "msg": msg})
        if notify:
            return None, line_req, None
        t0 = time.time()
        while True:
            out = self.p.stdout.readline()
            if not out:
                err = self.p.stderr.read()
                raise RuntimeError(f"server closed: {err}")
            resp = json.loads(out)
            if resp.get("id") == msg["id"]:
                break
            self.log({"dir": "<-", "note": "unrelated message", "msg": resp})
        dt = time.time() - t0
        shown = self.strip_images(resp, label or method)
        line_resp = self.log({"dir": "<-", "t": time.time(), "secs": round(dt, 3), "label": label, "msg": shown})
        return resp, line_req, line_resp

    def strip_images(self, resp, label):
        resp = json.loads(json.dumps(resp))
        content = resp.get("result", {}).get("content")
        if isinstance(content, list):
            for i, c in enumerate(content):
                if c.get("type") == "image":
                    data = base64.b64decode(c["data"])
                    safe = "".join(ch if ch.isalnum() or ch in "-_." else "_" for ch in label)
                    path = os.path.join(self.evidence, f"{safe}.png")
                    n = 2
                    while os.path.exists(path):
                        path = os.path.join(self.evidence, f"{safe}-{n}.png")
                        n += 1
                    with open(path, "wb") as f:
                        f.write(data)
                    c["data"] = f"<{len(data)} bytes saved to {path}>"
                    c["saved"] = path
                    self.saved.append(path)
        return resp

    def init(self):
        resp, _, _ = self.send("initialize", {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "dogfood3-agent", "version": "0.1"},
        }, label="initialize")
        self.send("notifications/initialized", notify=True, label="initialized")
        return resp

    def close(self):
        try:
            self.p.stdin.close()
            self.p.wait(timeout=10)
        except Exception:
            self.p.kill()


SAVED = []


def show(resp, quiet=False, limit=6000):
    r = resp.get("result")
    if r is None:
        print(json.dumps(resp, indent=1)[:limit])
        return
    if "content" in r:
        for c in r["content"]:
            if c.get("type") == "text":
                t = c["text"]
                if quiet:
                    t = t[:600]
                print(("ERROR: " if r.get("isError") else "") + t[:limit])
                if len(t) > limit:
                    print(f"... [{len(t)} chars total]")
            elif c.get("type") == "image":
                print("IMAGE:", SAVED.pop(0) if SAVED else "?")
        if "structuredContent" in r and not any(c.get("type") == "text" for c in r["content"]):
            print(json.dumps(r["structuredContent"], ensure_ascii=False)[:limit])
    else:
        print(json.dumps(r, ensure_ascii=False, indent=1)[:limit])


def main():
    args = sys.argv[1:]
    task, cmd = args[0], args[1]
    rest = args[2:]
    opts = {}
    pos = []
    i = 0
    while i < len(rest):
        if rest[i].startswith("--"):
            key = rest[i][2:]
            if key in ("quiet",):
                opts[key] = True
                i += 1
            else:
                opts[key] = rest[i + 1]
                i += 2
        else:
            pos.append(rest[i])
            i += 1
    s = Server(task)
    s.init()
    limit = int(opts.get("limit", 6000))
    try:
        if cmd == "list-tools":
            resp, a, b = s.send("tools/list", {}, label="tools/list")
            if "save" in opts:
                with open(opts["save"], "w") as f:
                    json.dump(resp["result"], f, indent=1, ensure_ascii=False)
            for t in resp["result"]["tools"]:
                print(t["name"])
            print(f"[transcript lines {a}-{b}]")
        elif cmd == "list-resources":
            resp, a, b = s.send("resources/list", {}, label="resources/list")
            for r in resp["result"]["resources"]:
                print(r["uri"], "|", r.get("name"), "|", r.get("size"))
            print(f"[transcript lines {a}-{b}]")
        elif cmd == "read":
            resp, a, b = s.send("resources/read", {"uri": pos[0]}, label=f"read {pos[0]}")
            if "error" in resp:
                print(json.dumps(resp["error"]))
            else:
                text = resp["result"]["contents"][0]["text"]
                if "save" in opts:
                    with open(opts["save"], "w") as f:
                        f.write(text)
                    print(f"saved {len(text)} chars to {opts['save']}")
                else:
                    print(text[:limit])
            print(f"[transcript lines {a}-{b}]")
        elif cmd == "call":
            tool, jargs = pos[0], json.loads(pos[1])
            label = opts.get("name", tool)
            resp, a, b = s.send("tools/call", {"name": tool, "arguments": jargs}, label=label)
            SAVED.extend(s.saved)
            if "save" in opts:
                with open(opts["save"], "w") as f:
                    json.dump(resp, f, indent=1, ensure_ascii=False)
            show(resp, quiet=opts.get("quiet", False), limit=limit)
            print(f"[transcript {s.transcript} lines {a}-{b}]")
        elif cmd == "calls":
            with open(pos[0]) as f:
                calls = json.load(f)
            for c in calls:
                tool, jargs = c[0], c[1]
                label = c[2] if len(c) > 2 else tool
                s.saved = []
                resp, a, b = s.send("tools/call", {"name": tool, "arguments": jargs}, label=label)
                SAVED.extend(s.saved)
                print(f"== {label} [lines {a}-{b}]")
                show(resp, quiet=opts.get("quiet", False), limit=limit)
        else:
            raise SystemExit(f"unknown command {cmd}")
    finally:
        s.close()


if __name__ == "__main__":
    main()
