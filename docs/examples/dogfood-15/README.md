# Dogfood 15: a Japanese keynote with pictures, through the CLI

This dogfood made a Japanese 10-minute keynote with `scaena generate`, its goal Inspire, for local librarians and city staff. The slides were set as photographs (`--pictures openai --picture-look photo`). The run used scripted Anthropic and OpenAI image endpoints on this machine. The deck was judged on:

- its pictures, and a picture refused in the last round;
- the cover and close standing on the theme's backdrop;
- Japanese typesetting;
- the notes timed at 300 characters a minute;
- health read in Japanese.

Built at main 029dae92: the release CLI.

## How it was run

`scripted.py` stands in for both providers on this machine. It answers from a script:

- **The arc:** The Reveal.
- **The slides:** eight, written in Japanese:
  - a cover;
  - a Statement whose claim quotes 「静かにしなければならない」;
  - a Big Number, 32%;
  - a divider;
  - three points, one of them "毎週120キロ";
  - a Quote written with the quote as its title and the speaker as its one point;
  - a line chart in 万人;
  - a close that says "ありがとうございました" over "ご質問をどうぞ".
- **The look:** no changes.
- **The notes:** as many characters as each slide is asked for. Where a slide is asked in words, about two characters a word.
- **The fit:** under `NO_FIT=1` it rewrites nothing, so what runs long is seen as the builder set it.
- **The pictures:** each is `picture.jpg`. The close's picture, whose prompt names 夕暮れ, is answered busy (429) the first time. The second time it is refused by the safety system (400, `moderation_blocked`).

```
python3 -I scripted.py LOGDIR 8790 &
ANTHROPIC_API_KEY=sk-ant-dummy OPENAI_API_KEY=sk-dummy scaena generate OUT \
  "小さな町の図書館が、なぜ若者で満員になったのか" \
  --provider anthropic --model claude-test --base http://127.0.0.1:8790 \
  --picture-base http://127.0.0.1:8790 --pictures openai --picture-look photo \
  --language Japanese --minutes 10 --goal Inspire --audience "地域の図書館員と市の職員"
```

## Findings, worst first

Each finding has its own pull request off main.

1. **No Japanese notes were kept (#1196).** All 8 slides' notes were written, and all 8 were dropped as too short for their share of the talk. Every beat stayed at Narrato's 15-second estimate. Two parts disagreed:
   - The notes' prompt asked each slide in words, at 145 a minute ("about 60 words").
   - `notes::counted` split on spaces, so a slide's 300 characters of Japanese counted as a word or two.

   Health already times a talk set without spaces at 300 characters a minute.
   - Now: a talk in Japanese, Chinese, or Korean is asked and counted in characters at 300 a minute. All 8 slides keep 263–329 characters of notes, each beat is timed 69–76 s, and health says "About 10 minutes of talk, where its brief asks 10 minutes."
2. **A quote written as the slide's title was lost (#1197).** The Quote slide set 「館長 佐藤 花子」, the speaker's name in quotation marks, as the quote. The quote itself, 「ここは、静かにしなくていい図書館です」, was dropped without a word. The model had put the quote in `slideTitle` and the speaker in the one bullet, where the slot shapes ask for the words first and who said them second.
   - Now: a title wholly in quotation marks, over one point in none, is the quote, and the point is who said it.
   - A quote in 「」 is set once in the deck's own marks, not in two pairs.
3. **The close cut its one word in two (#1201).** ありがとうございました was set ありがとうご / ざいました on a slide 1680 wide. The display role's `measure` is 16 of its zeros, about nine kana, and a Japanese phrase too wide for its line breaks where it must (`overflow-wrap: anywhere`). An English word longer than the measure stands whole past it, and so the eleven kana would have stood whole too.
   - Now: a Japanese text's lines break as wide as its widest phrase, where that is wider than the measure, up to the box. A phrase is cut only where the box cannot hold it. The close stands on one line.
   - Also: a text that shrinks or grows in Japanese is set as large as keeps each phrase whole within its box. A break after a particle (本当に / うまく) counts as no cut. Where no size above its floor keeps a phrase whole, it is set as large as it fits, as before.
4. **The line's callout says "4.8", its unit only in the axis title (no PR).** The chart's values are in 万人, which the subtitle names: 年間来館者数（万人）. The callout is the line's latest value, and the note under it says "2025年に4.8万人". An English chart in thousands reads the same way, so this is recorded, not fixed.

## What held

- **The pictures.**
  - Four slides were asked for a picture: the cover, the divider, the three points, and the close. That is two in five of eight, the cover, divider, and close first.
  - Each prompt asks for editorial photography in the theme's palette, with no text in it.
  - Three were made (`assets/picture-01.jpg`, `picture-04.jpg`, `picture-05.jpg`). Each is set beside its words in a layout that holds one: `title-split`, `section-photo`, and `bullets-photo`.
- **A refusal in the last round says why.** The close's picture was busy, asked again, then refused. The run says "picture not made: slide 8: OpenAI's safety system refused it: 400 Bad Request: Your request was rejected by the safety system."
- **The close on the backdrop.** With no picture, the close stands on the theme's `backdrop` shader, and its words read over it.
- **Japanese typesetting.**
  - **Fonts:** Noto Sans JP was fetched and held to its SHA-256 before any call.
  - **Breaks:** the cover breaks 図書館は、/ 本当にうまくいく after its comma. The Statement breaks between its phrases.
  - **Figures:** set in the script's face: 2026年, 10代は8%, 32%.
  - **Quotes and units:** quotations stand in 「…」, and "毎週120キロ" keeps its kana unit against its figure.
- **The fit says what it could not shorten.** "not rewritten to fit: 2 texts still run past their place", naming the Statement's claim and the cover's title. Lint finds the Statement past its box (E100), and health caps its score at 78 while slide 2 has that error.
- **Health in Japanese:**
  - **Pacing:** 100: "8 slides, as the brief asked (5–10). About 10 minutes of talk."
  - **Coherence:** not read: "health reads words a space sets apart, and this deck is in Japanese". So no turn was found.
  - **Intensity and tone:** no score: "health reads it only in English, and this deck is in Japanese".
  - **Arc:** one information finding, slide 3 planned hotter than its place, with a fix that plans it at 0.39.
  - **Claims and headlines:** no claim read and no headline asked for its point.
