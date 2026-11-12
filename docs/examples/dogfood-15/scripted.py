# W1 dogfood 15: Anthropic and OpenAI's image endpoint, scripted, on this machine.
#   python3 -I scripted.py LOGDIR [PORT]
# A Japanese 10-minute keynote: the arc, the slides, the look, the notes, and the fit; pictures
# made from a JPEG, the closing slide's busy once then refused by the safety system.
import base64, json, os, re, sys, threading
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
OUT = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8790
HERE = os.path.dirname(os.path.abspath(__file__))
PICTURE = base64.b64encode(open(os.path.join(HERE, "picture.jpg"), "rb").read()).decode()
lock = threading.Lock(); seq = [0]; closes = [0]
os.makedirs(OUT, exist_ok=True)

SLIDES = {
  "executiveSummary": "静かでなくていい図書館が、町の若者を呼び戻した。",
  "slides": [
    {"slideTitle": "図書館は、本当にうまくいく", "subtitle": "2026年 地域フォーラム 基調講演", "layoutSuggestion": "Title", "keyBullets": [],
     "visualDescription": "朝の光が差し込む、木の書架が並ぶ明るい図書館", "beatIndex": 0, "expectedIntensity": 0.4, "expectedValence": 0.5},
    {"slideTitle": "「静かにしなければならない」という思い込みが、若者を遠ざけていた", "subtitle": "", "layoutSuggestion": "Statement",
     "keyBullets": ["2019年の来館者のうち、10代は8%だった"], "beatIndex": 0, "expectedIntensity": 0.5, "expectedValence": 0.0},
    {"slideTitle": "来館者の3割が10代になった", "subtitle": "", "layoutSuggestion": "Big Number",
     "keyBullets": ["32%", "の来館者が、いまは10代"], "beatIndex": 1, "expectedIntensity": 0.7, "expectedValence": 0.6},
    {"slideTitle": "第2部：変えたこと", "subtitle": "三つの小さな決断", "layoutSuggestion": "Section Header", "keyBullets": [],
     "visualDescription": "話し声と笑い声があふれる、ソファの並んだ閲覧室", "beatIndex": 2, "expectedIntensity": 0.5, "expectedValence": 0.4},
    {"slideTitle": "三つの小さな決断", "subtitle": "", "layoutSuggestion": "Bullet List",
     "keyBullets": ["話してよいフロアを2階に設けた", "毎週120キロの本を移動図書館で運んだ", "閉館を夜9時まで延ばした"], "beatIndex": 2, "expectedIntensity": 0.6, "expectedValence": 0.5},
    {"slideTitle": "「ここは、静かにしなくていい図書館です」", "subtitle": "", "layoutSuggestion": "Quote",
     "keyBullets": ["館長 佐藤 花子"], "beatIndex": 3, "expectedIntensity": 0.8, "expectedValence": 0.8},
    {"slideTitle": "来館者は5年で倍になった", "subtitle": "年間来館者数（万人）", "layoutSuggestion": "Line Chart",
     "keyBullets": ["2025年に4.8万人"], "beatIndex": 3, "expectedIntensity": 0.7, "expectedValence": 0.7,
     "chartData": {"categories": ["2021", "2022", "2023", "2024", "2025"], "series": [{"name": "来館者数", "values": [2.4, 2.9, 3.5, 4.1, 4.8]}]}},
    {"slideTitle": "ありがとうございました", "subtitle": "", "layoutSuggestion": "Closing", "keyBullets": ["ご質問をどうぞ"],
     "visualDescription": "夕暮れの図書館の窓から見える町並み", "beatIndex": 4, "expectedIntensity": 0.3, "expectedValence": 0.6},
  ],
}

def text(t): return {"content": [{"type": "text", "text": t if isinstance(t, str) else json.dumps(t, ensure_ascii=False)}], "stop_reason": "end_turn"}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def reply(self, status, body, headers=None):
        raw = json.dumps(body, ensure_ascii=False).encode()
        self.send_response(status); self.send_header("content-type", "application/json")
        for k, v in (headers or {}).items(): self.send_header(k, v)
        self.send_header("content-length", str(len(raw))); self.end_headers(); self.wfile.write(raw)
    def do_GET(self):
        if self.path.startswith("/v1/models"):
            return self.reply(200, {"data": [{"id": "gpt-image-1"}, {"id": "dall-e-3"}, {"id": "claude-test", "display_name": "Scripted"}]})
        self.reply(404, {"error": {"message": "no " + self.path}})
    def do_POST(self):
        n = int(self.headers.get("content-length", 0)); body = json.loads(self.rfile.read(n) or b"{}")
        with lock:
            seq[0] += 1; k = seq[0]
        if self.path.startswith("/v1/images/generations"):
            prompt = body.get("prompt", "")
            open(f"{OUT}/req-{k:02d}-picture.txt", "w").write(json.dumps(body, ensure_ascii=False, indent=1))
            if "夕暮れ" in prompt:
                with lock:
                    closes[0] += 1; c = closes[0]
                if c == 1:
                    return self.reply(429, {"error": {"message": "Rate limit reached for images", "code": "rate_limit_exceeded"}}, {"retry-after": "1"})
                return self.reply(400, {"error": {"message": "Your request was rejected by the safety system.", "code": "moderation_blocked"}})
            return self.reply(200, {"data": [{"b64_json": PICTURE}]})
        system = " ".join(s.get("text", "") for s in body.get("system", []) or [])
        msg = body["messages"][0]["content"]
        prompt = "\n".join(c.get("text", "") for c in msg if c.get("type") == "text") if isinstance(msg, list) else msg
        asked = ("fit" if "tightening the words" in system else "notes" if "speechwriter" in system else
                 "look" if "art director" in system else "outline" if "planning a presentation before its slides" in system else
                 "arc" if "=== CATALOG ===" in prompt else "slides")
        open(f"{OUT}/req-{k:02d}-{asked}.txt", "w").write("SYSTEM: " + system[:400] + "\n\n" + prompt)
        if asked == "arc":
            return self.reply(200, text({"selection": "The Reveal", "reasoning": "基調講演は、思い込みを覆す発見で聴衆を引き込む。", "candidates": [{"arc": "The Reveal", "score": {"overall": 0.9}}]}))
        if asked == "slides": return self.reply(200, text(SLIDES))
        if asked == "look": return self.reply(200, text({"slides": []}))
        if asked == "notes":
            notes = []
            for m in re.finditer(r"Slide (\d+) of \d+: about (\d+) (words|characters)\.", prompt):
                s, w, unit = int(m.group(1)), int(m.group(2)), m.group(3)
                # As many characters as asked; for words, about two characters each.
                n = w if unit == "characters" else w * 2
                notes.append({"slide": s, "notes": ("この町の図書館は、変わることを恐れませんでした。" * 80)[: max(10, n)]})
            return self.reply(200, text({"notes": notes}))
        # The fit rewrites nothing here, so what runs long is seen as the builder set it.
        if os.environ.get("NO_FIT"): return self.reply(200, text({"texts": []}))
        out = []
        for m in re.finditer(r"id: (.+)\nat most: (\d+) characters\ntext:\n<data label=\"text\">\n([^\n]*)", prompt):
            out.append({"id": m.group(1), "text": m.group(3)[: max(1, int(m.group(2)) - 1)]})
        self.reply(200, text({"texts": out}))

ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
