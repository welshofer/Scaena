import json, re, sys, time, threading
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
OUT = sys.argv[1]
lock = threading.Lock(); seq = [0]; T0 = time.time()
# A 15-minute developer talk on hyperfine, sharkdp/hyperfine at c5161524, on the Learning Journey.
CLAIMS = {
 "1": "A benchmark you run once tells you nothing; hyperfine runs it until the numbers settle.",
 "2": "Hyperfine runs every command at least 10 times and aims for about 3 seconds of measurement.",
 "3": "Warmup runs and prepare commands decide whether you measure a warm cache or a cold one.",
 "4": "Run fast commands without a shell, and hyperfine has no startup to correct for.",
 "5": "In hyperfine's own example, fd finds the files 9.8x faster than find.",
 "6": "Compare your branches with a parameter list and a setup command, not by hand.",
}
IDEA = "Measure a command until its numbers settle, and compare it against a reference, never once by hand."
OUTLIER = "pub const OUTLIER_THRESHOLD: f64 = 1.4826 * 10.0;\n\npub fn num_outliers(xs: &[f64]) -> usize {\n    if xs.is_empty() {\n        return 0;\n    }\n    let scores = modified_zscores_f64(xs);\n    scores.iter().filter(|&&s| s.abs() > OUTLIER_THRESHOLD).count()\n}"
PREPARE = "hyperfine -S --warmup 3 'grep -R TODO *'\nhyperfine -S --prepare 'sync; echo 3 | sudo tee /proc/sys/vm/drop_caches' 'grep -R TODO *'"
COMPARE = "hyperfine \\\n    --parameter-list branch main,performance-improvements \\\n    --setup 'git switch {branch}' \\\n    'python main.py'"
def beat_slides(k, claim):
    """Each beat's slides, in order, as the slides' prompt asks for them."""
    statement = dict(slideTitle=claim.rstrip("."), layoutSuggestion="Statement")
    return [
        [dict(slideTitle="Benchmark like you mean it", subtitle="Measuring command-line performance with hyperfine", layoutSuggestion="Title", expectedIntensity=0.4),
         statement],
        [statement,
         dict(slideTitle="Hyperfine picks the run count for you", layoutSuggestion="Code", subtitle="run.sh",
              keyBullets=["hyperfine 'sleep 0.3'\nhyperfine --runs 5 'sleep 0.3'\nhyperfine 'hexdump file' 'xxd file'"])],
        [statement,
         dict(slideTitle="Warm cache or cold", layoutSuggestion="Two Column",
              keyBullets=["Warm: --warmup 3 runs the command three times before measuring", "Cold: --prepare clears the page cache before each run"]),
         dict(slideTitle="One flag for each", layoutSuggestion="Code", subtitle="cache.sh", keyBullets=[PREPARE])],
        [statement,
         dict(slideTitle="What the shell costs", layoutSuggestion="Bullet List",
              keyBullets=["Commands run directly by default: --shell=none", "Use -S for pipes, redirections, and wildcards", "With a shell, hyperfine subtracts its startup time", "Below 5 ms, that correction is noise of its own"]),
         dict(slideTitle="An outlier is ten deviations out", layoutSuggestion="Code", subtitle="src/outlier_detection.rs", keyBullets=[OUTLIER], expectedIntensity=0.6)],
        [dict(slideTitle="fd finds the files 9.8x faster than find", layoutSuggestion="Bar Chart",
              keyBullets=["Mean wall time in seconds, from hyperfine's README"],
              chartData={"categories": ["find -iregex", "find -iname", "fd"], "series": [{"name": "Mean wall time (s)", "values": [2.275, 1.427, 0.232]}]},
              expectedIntensity=0.8, expectedValence=0.6),
         dict(slideTitle="Check yourself", layoutSuggestion="Quiz",
              keyBullets=["Which option benchmarks a cold cache?", "A: --warmup", "B: --prepare", "C: --runs"])],
        [statement,
         dict(slideTitle="Benchmark two branches in one command", layoutSuggestion="Code", subtitle="compare.sh", keyBullets=[COMPARE]),
         dict(slideTitle="Thank you", layoutSuggestion="Closing", keyBullets=["Questions?", "github.com/sharkdp/hyperfine"], expectedIntensity=0.4)],
    ][k]
def slides_for(prompt):
    beats = re.findall(r'Beat (\d+) "([^"]+)" → (?:slide (\d+)|slides (\d+)–(\d+)): (.*)', prompt)
    out = []
    for k, (i, name, one, a, b, claim) in enumerate(beats):
        n = 1 if one else int(b) - int(a) + 1
        for j, written in enumerate(beat_slides(k, claim)[:n]):
            s = {"beatIndex": int(i) - 1, "beatName": name, "subtitle": "", "keyBullets": [], "expectedIntensity": 0.5, "expectedValence": 0.3, "sourceRef": "hyperfine.md"}
            s.update(written)
            out.append(s)
    return {"executiveSummary": IDEA, "slides": out}
def look_for(prompt):
    ch = []
    for m in re.finditer(r"Slide (\d+) of \d+, drawn as [^\n]*\n<data[^>]*>\nTitle: ([^\n]*)", prompt):
        n, title = int(m.group(1)), m.group(2)
        if title == "Warm cache or cold":
            # A topic made a claim, as an art director would.
            ch.append({"slide": n, "layout": "", "title": "Warm the cache, or clear it before every run", "subtitle": "", "bullets": [], "why": "topic, not claim"})
    return {"slides": ch}
def fit_for(prompt):
    return {"texts": []}
def notes_for(prompt):
    filler = "Here is what that means when you sit down to measure your own tool and why the number on the screen is only as good as the runs behind it".split()
    notes = []
    for m in re.finditer(r"Slide (\d+) of \d+: about (\d+) words\.", prompt):
        n, b = int(m.group(1)), int(m.group(2))
        words = (filler * (b // len(filler) + 2))[:b]
        notes.append({"slide": n, "notes": " ".join(words).capitalize() + "."})
    return {"notes": notes}
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        t_in = time.time() - T0
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        system = body.get("system")
        if isinstance(system, list): system = " ".join(x.get("text", "") for x in system)
        sy = system or ""
        texts = []
        for m in body.get("messages", []):
            c = m["content"]
            texts += [c] if isinstance(c, str) else [x.get("text", "") for x in c if x.get("type") == "text"]
        prompt = "\n".join(texts)
        kind = ("look" if "art director" in sy else "fit" if "tightening the words" in sy else
                "arc" if "narrative arc" in sy else "outline" if "planning a presentation" in sy else
                "slides" if "generating a presentation" in sy else "notes" if "speechwriter" in sy else "other")
        with lock: seq[0] += 1; k = seq[0]
        if kind == "arc":
            answer = {"selection": "learning-journey", "reasoning": "A talk that teaches a tool.", "candidates": [{"arc": "Learning Journey", "score": {"overall": 0.9}}]}
        elif kind == "outline":
            beats = re.findall(r'Beat (\d+) "([^"]+)"', prompt)
            answer = {"bigIdea": IDEA, "beats": [{"beat": int(b), "claim": CLAIMS.get(str(i + 1), CLAIMS["6"])} for i, (b, _) in enumerate(beats)]}
        elif kind == "slides": answer = slides_for(prompt)
        elif kind == "look": answer = look_for(prompt)
        elif kind == "fit": answer = fit_for(prompt)
        elif kind == "notes": answer = notes_for(prompt)
        else: answer = {}
        text = json.dumps(answer, ensure_ascii=False)
        out = {"id": f"msg_{k}", "type": "message", "role": "assistant", "model": body.get("model"),
               "content": [{"type": "text", "text": text}], "stop_reason": "end_turn", "usage": {"input_tokens": 1, "output_tokens": 1}}
        data = json.dumps(out).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data)
        with open(f"{OUT}/req-{k:02d}-{kind}.txt", "w") as f: f.write("SYSTEM: " + sy[:300] + "\n\n" + prompt + "\n\n=== ANSWER ===\n" + text)
        print(f"{t_in:7.1f}s #{k} {kind}", flush=True)
ThreadingHTTPServer(("127.0.0.1", 8787), H).serve_forever()
