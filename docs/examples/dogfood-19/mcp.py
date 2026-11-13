# A minimal MCP client over stdio: one call per line of a plan, results to calls/NN-tool.json.
import json, subprocess, sys, os
exe, work, plan = sys.argv[1], sys.argv[2], sys.argv[3]
p = subprocess.Popen([exe, "mcp"], cwd=work, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env={**os.environ, "SCAENA_AUTHOR": "agent:dogfood"})
n = 0
def rpc(method, params=None, note=False):
    global n
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None: msg["params"] = params
    if not note:
        n += 1; msg["id"] = n
    p.stdin.write(json.dumps(msg) + "\n"); p.stdin.flush()
    if note: return None
    while True:
        line = p.stdout.readline()
        if not line: raise SystemExit("server closed")
        r = json.loads(line)
        if r.get("id") == n: return r
rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "dogfood", "version": "1"}})
rpc("notifications/initialized", note=True)
os.makedirs(os.path.join(work, "calls"), exist_ok=True)
for i, call in enumerate(json.load(open(plan))):
    r = rpc("tools/call", {"name": call["tool"], "arguments": call["args"]})
    out = os.path.join(work, "calls", f"{i:02d}-{call['tool']}.json")
    json.dump(r, open(out, "w"), ensure_ascii=False, indent=1)
    res = r.get("result", {})
    text = "".join(c.get("text", "") for c in res.get("content", []) if c.get("type") == "text")
    print(f"== {i} {call['tool']} error={res.get('isError')} {text[:300]!r}")
p.stdin.close(); p.wait()
