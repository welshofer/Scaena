"""Writes brief.pptx: Hikari Mobility's product brief for the Sora Cargo, words only, in Japanese.

The deck dogfood 20 makes is grounded in it beside preorders.csv. Run from this folder:
    python3 brief.py
"""

from pptx import Presentation
from pptx.util import Inches, Pt

SLIDES = [
    ("ソラカーゴ 製品概要", ["ヒカリモビリティ株式会社", "販売店パートナー向け資料 2026年春"]),
    ("市場", ["都市部の宅配のうち、配達先まで5km以内の荷物は全体の40%", "配送用の軽バンの平均稼働率は38%にとどまる", "駐車できず、荷物1件あたり平均4分を探す時間に使っている"]),
    ("製品仕様", ["価格：298,000円（税込）", "航続距離：80km（標準バッテリー）", "最大積載量：120kg", "充電時間：4時間", "発売日：2026年4月15日"]),
    ("実証実験（横浜市、2025年）", ["1日あたりの配達件数は軽バンの1.8倍", "1台あたりの燃料費を年間42万円削減", "参加した配達員12名のうち11名が継続利用を希望"]),
    ("予約状況", ["2026年3月末時点の予約は累計3,200台", "契約済みの販売店は52店"]),
    ("販売店の条件", ["販売店の粗利率は22%", "試乗車1台を発売前に提供", "整備研修は2日間、全国6会場で実施"]),
]

deck = Presentation()
deck.slide_width, deck.slide_height = Inches(13.333), Inches(7.5)
for title, lines in SLIDES:
    slide = deck.slides.add_slide(deck.slide_layouts[1])
    slide.shapes.title.text = title
    body = slide.placeholders[1].text_frame
    body.text = lines[0]
    for line in lines[1:]:
        body.add_paragraph().text = line
    for p in body.paragraphs:
        for r in p.runs:
            r.font.size = Pt(24)
deck.save("brief.pptx")
