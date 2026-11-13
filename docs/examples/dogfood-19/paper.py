# Writes a fictional French research paper as a PDF, for W1 dogfood 19.
from reportlab.lib.pagesizes import A4
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib import colors
import sys

pdfmetrics.registerFont(TTFont("Serif", "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf"))
pdfmetrics.registerFont(TTFont("SerifB", "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"))
NN = " "  # espace fine insécable
body = ParagraphStyle("b", fontName="Serif", fontSize=10.5, leading=14)
helv = ParagraphStyle("h", fontName="Helvetica", fontSize=10.5, leading=14)
h1 = ParagraphStyle("t", fontName="SerifB", fontSize=17, leading=22)
h2 = ParagraphStyle("s", fontName="SerifB", fontSize=12.5, leading=17, spaceBefore=10)

rows = [
    ("Quartier", "Toitures végétalisées (m²)", "Baisse de température (°C)", "Part de la surface (%)"),
    ("Confluence", f"184{NN}500", "2,4", "11,2"),
    ("Part-Dieu", f"97{NN}300", "1,6", "6,8"),
    ("Croix-Rousse", f"61{NN}200", "1,1", "4,3"),
    ("Gerland", f"142{NN}800", "1,9", "8,9"),
    ("Vaise", f"38{NN}400", "0,7", "3,7"),
]

story = [
    Paragraph("Toitures végétalisées et îlots de fraîcheur&nbsp;: une étude de cinq quartiers lyonnais", h1),
    Spacer(1, 6),
    Paragraph("Claire Duvernois, Mathieu Roussel — Laboratoire d’écologie urbaine (étude fictive)", body),
    Paragraph("Résumé", h2),
    Paragraph(
        "Nous mesurons l’eﬃcacité des toitures végétalisées sur la température de surface "
        f"de cinq quartiers de Lyon pendant l’été 2025. Sur 1{NN}240{NN}000 relevés horaires, "
        f"chaque tranche de 100{NN}000{NN}m² de toiture abaisse la température de 1,3{NN}°C en moyenne. "
        "La ﬂore choisie compte autant que la surface : les sédums résistent, les graminées "
        "se dessèchent. Le cœur de notre propos : verdir les toits est la réﬂexion la plus "
        f"rentable à l’échelle d’un quartier, pour 3,7{NN}% de la surface bâtie.", body),
    Paragraph("1. Méthode", h2),
    Paragraph(
        "Les capteurs ont été posés sur 412 toits entre juin et septembre. L’œuvre de terrain "
        "a duré quatorze semaines ; chaque relevé est horodaté et géolocalisé.", helv),
    Paragraph("2. Résultats", h2),
    Paragraph("Tableau 1 — Baisse moyenne de la température de surface par quartier, été 2025.", body),
    Spacer(1, 4),
]
t = Table(rows, colWidths=[90, 135, 135, 110])
t.setStyle(TableStyle([
    ("FONT", (0, 0), (-1, -1), "Serif", 9.5),
    ("FONT", (0, 0), (-1, 0), "SerifB", 9.5),
    ("LINEBELOW", (0, 0), (-1, 0), 0.6, colors.black),
    ("ALIGN", (1, 1), (-1, -1), "RIGHT"),
]))
story += [t, Spacer(1, 8),
    Paragraph(
        "Confluence, le quartier le plus végétalisé, gagne 2,4 °C ; Vaise, le moins couvert, "
        "à peine 0,7 °C. L’écart tient moins à la surface qu’à la ﬂore : à Gerland, les sédums "
        "ont survécu à la canicule d’août.", body),
    Paragraph("3. Discussion", h2),
    Paragraph(
        "Pourquoi ces écarts ? Parce qu’une toiture sèche ne rafraîchit rien ! Nous recommandons "
        "« l’arrosage d’appoint » en période de canicule, et un suivi sur trois étés.", body),
    Paragraph("Références", h2),
    Paragraph("Bernard, A. et Lefèvre, M. (2023). <i>Végétaliser la ville</i>. Presses universitaires de Lyon.", body),
    Paragraph("Oke, T. R. (1982). The energetic basis of the urban heat island. <i>Quarterly Journal of the Royal Meteorological Society</i>, 108, 1–24.", body),
    Paragraph("Roussel, M. (2024). Capteurs à bas coût et îlots de chaleur. <i>Revue d’écologie urbaine</i>, 12(3), 45–61.", body),
]
SimpleDocTemplate(sys.argv[1], pagesize=A4, title="Toitures végétalisées et îlots de fraîcheur", author="C. Duvernois").build(story)
