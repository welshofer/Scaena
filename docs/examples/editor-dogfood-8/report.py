# Writes report.pdf, editor dogfood 8's source: a water utility's lead service line inventory,
# with a text layer the generator's PDF reader takes the words and figures from.
#   python3 -I docs/examples/editor-dogfood-8/report.py
from pathlib import Path
from reportlab.lib.pagesizes import letter
from reportlab.lib.styles import getSampleStyleSheet
from reportlab.platypus import Paragraph, SimpleDocTemplate, Spacer

out = Path(__file__).with_name("report.pdf")
styles = getSampleStyleSheet()
body = [
    ("Title", "Clearwater Lead Service Line Inventory, 2026"),
    ("Normal", "Prepared by the Clearwater Water Department for the Board of Water Commissioners."),
    ("Heading2", "Where the lead is"),
    ("Normal", "Clearwater still carries drinking water to homes through lead service lines. The 2026 inventory "
     "walked every connection in the city. Eastgate holds 31% of the lead lines that remain, the most of any "
     "neighborhood, and its homes are the oldest in the city."),
    ("Heading2", "What it costs"),
    ("Normal", "Replacing a lead service line costs $9,800 on average, the street side and the home side together. "
     "Crews replaced the most lines in the department's history last year, but at that pace the work would "
     "not finish until 2034."),
    ("Heading2", "What the money is"),
    ("Normal", "Federal and state programs hold $42 million for Clearwater's lines through 2030. The federal rule "
     "requires every lead line replaced by 2037."),
    ("Heading2", "The proposal"),
    ("Normal", "The department proposes to replace every remaining lead line by 2032: 3,700 lines a year, "
     "Eastgate first, with the grants covering most of the cost."),
]
doc = SimpleDocTemplate(str(out), pagesize=letter, title="Clearwater Lead Service Line Inventory, 2026", author="Clearwater Water Department")
story = []
for style, text in body:
    story += [Paragraph(text, styles[style]), Spacer(1, 8)]
doc.build(story)
print(out)
