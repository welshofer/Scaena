import json, re, sys, time, threading
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
OUT = sys.argv[1]
lock = threading.Lock(); seq = [0]; T0 = time.time()
CLAIMS = {
 "1": "Speisereste sind ein Drittel unseres Restmülls, und jede Tonne wird teurer.",
 "2": "Die Deponie schließt 2034, und jedes Jahr des Wartens erhöht die Rechnung.",
 "3": "Die Biotonne holt Speisereste auf den Touren ab, die wir ohnehin fahren.",
 "4": "Das Pilotprojekt senkte die Speisereste im Restmüll um 58 % in einem Jahr.",
 "5": "Bewilligen Sie 2,4 Mio. € für die Tonnen im Haushalt 2026.",
}
IDEA = "Die Biotonne rechnet sich in drei Jahren und hält ein Drittel unseres Mülls von der Deponie fern."
def slides_for(prompt):
    beats = re.findall(r'Beat (\d+) "([^"]+)" → (?:slide (\d+)|slides (\d+)–(\d+)): (.*)', prompt)
    out = []
    for k, (i, name, one, a, b, claim) in enumerate(beats):
        n = 1 if one else int(b) - int(a) + 1
        bi = int(i) - 1
        last_beat = k == len(beats) - 1
        for j in range(n):
            s = {"beatIndex": bi, "beatName": name, "subtitle": "", "keyBullets": [], "expectedIntensity": 0.5, "expectedValence": 0.2}
            if k == 0 and j == 0:
                s.update(slideTitle="Biotonne für alle Haushalte bis 2028", subtitle="Vorlage für den Stadtrat", layoutSuggestion="Title")
            elif last_beat and j == n - 1:
                s.update(slideTitle="Vielen Dank", layoutSuggestion="Closing", keyBullets=["Fragen?", "biotonne@lindenau.de"], expectedIntensity=0.4)
            elif name == "Einwände":
                # A beat whose slides drift from its claim: they share none of its words.
                s.update(slideTitle="Wie der Winterdienst organisiert ist", layoutSuggestion="Bullet List",
                         keyBullets=["Räumfahrzeuge: zwölf im Stadtgebiet", "Streusalz: Lager an der Nordstraße", "Einsatzplan: ab vier Uhr morgens"])
            elif j == 0:
                s.update(slideTitle=claim.rstrip("."), layoutSuggestion="Statement", keyBullets=["Quelle: Abfallbericht 2025"], sourceRef="bericht.pdf")
            elif "34 %" in claim or "Drittel" in claim:
                s.update(slideTitle="Speisereste im Restmüll", layoutSuggestion="Big Number", keyBullets=["34 %", "des Restmülls sind Speisereste, nach Gewicht"], sourceRef="bericht.pdf")
            elif "Deponie" in claim:
                s.update(slideTitle="Die Gebühren steigen jedes Jahr", layoutSuggestion="Line Chart", keyBullets=["Gebühr je Tonne, 2021 bis 2025"], sourceRef="abfall.csv",
                         chartData={"categories": ["2021","2022","2023","2024","2025"], "series": [{"name": "Gebühr je Tonne", "values": [48,52,57,63,71]}]})
            elif "Pilot" in claim:
                s.update(slideTitle="„Die Restmülltonne ist jetzt halb leer.“", layoutSuggestion="Quote", keyBullets=["Dana Weiß, Pilothaushalt in Ostend"])
            else:
                s.update(slideTitle="Kosten und Einsparungen", layoutSuggestion="Bullet List",
                         keyBullets=["Tonnen: 2,4 Mio. € einmalig", "Gebühren: 3,1 Mio. € pro Jahr vermieden", "Amortisation: drei Jahre"], sourceRef="bericht.pdf")
            out.append(s)
    return {"executiveSummary": IDEA, "slides": out}
def look_for(prompt):
    ch = []
    for m in re.finditer(r"Slide (\d+) of \d+, drawn as [^\n]*\n<data[^>]*>\nTitle: ([^\n]*)", prompt):
        n, title = int(m.group(1)), m.group(2)
        if title.startswith("Die Deponie schließt 2034"):
            # Tighter, and it drops words of the claim the person settled.
            ch.append({"slide": n, "layout": "", "title": "Die Deponie schließt 2034", "subtitle": "", "bullets": [], "why": "zu lang"})
        if title == "Speisereste im Restmüll":
            ch.append({"slide": n, "layout": "", "title": "", "subtitle": "", "bullets": ["34%", "des Restmülls sind Speisereste"], "why": "knapper"})
        if title == "Kosten und Einsparungen":
            ch.append({"slide": n, "layout": "", "title": "Die Tonnen kosten 2,4 Mio. € und sparen 3,1 Mio. € im Jahr", "subtitle": "", "bullets": [], "why": "Thema statt Aussage"})
    return {"slides": ch}
def fit_for(prompt):
    out = []
    for m in re.finditer(r"id: (\S+)\n[^\n]*?(\d+) characters[^\n]*\n(?:[^\n]*\n)*?text: ([^\n]*)", prompt):
        pass
    for block in prompt.split("id: ")[1:]:
        i = block.split("\n")[0].strip()
        if "deponie" in i: out.append({"id": i, "text": "Die Deponie schließt 2034: Warten wird teurer"})
        if "pilotprojekt" in i: out.append({"id": i, "text": "Pilot mit 4.200 Haushalten: 58 % weniger Speisereste"})
        if "biotonne-holt" in i: out.append({"id": i, "text": "Die Biotonne fährt auf unseren Touren mit"})
    return {"texts": out}
def notes_for(prompt):
    filler = "Wir sehen hier worauf es ankommt und was das für unsere Stadt bedeutet denn jede Entscheidung des Rates wirkt über Jahre hinweg auf alle Haushalte".split()
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
            answer = {"selection": "problem-solution", "reasoning": "Ein Problem und seine Lösung.", "candidates": [{"arc": "Problem-Solution", "score": {"overall": 0.9}}]}
        elif kind == "outline":
            beats = re.findall(r'Beat (\d+) "([^"]+)"', prompt)
            answer = {"bigIdea": IDEA, "beats": [{"beat": int(b), "claim": CLAIMS.get(str(i + 1), CLAIMS["5"])} for i, (b, _) in enumerate(beats)]}
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
