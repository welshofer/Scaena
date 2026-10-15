# The syllable table's sources and notices

`words.txt` beside this file, and `tests/verse/syllables.tsv`, are written by
`scripts/build_syllables.py` from two sources, and the rules in `../verse.rs` are written after a
third (PLAN O.4a, ADR-0030 decision 27).

## CMUdict

The Carnegie Mellon Pronouncing Dictionary, `cmudict.dict` at commit
74790861f652b15e4ac49015a90074ad62a27690 of https://github.com/cmusphinx/cmudict, SHA-256
81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22. Each word's syllables are the
phones of its pronunciations that carry a stress. Its license, as that commit's `LICENSE` gives it:

```
Copyright (C) 1993-2015 Carnegie Mellon University. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
   The contents of this file are deemed to be source code.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in
   the documentation and/or other materials provided with the
   distribution.

This work was supported in part by funding from the Defense Advanced
Research Projects Agency, the Office of Naval Research and the National
Science Foundation of the United States of America, and by member
companies of the Carnegie Mellon Sphinx Speech Consortium. We acknowledge
the contributions of many volunteers to the expansion and improvement of
this dictionary.

THIS SOFTWARE IS PROVIDED BY CARNEGIE MELLON UNIVERSITY ``AS IS'' AND
ANY EXPRESSED OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL CARNEGIE MELLON UNIVERSITY
NOR ITS EMPLOYEES BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## Google Books Ngram

The words are ranked by their matches since 1950 in the English 1-grams of Google Books Ngram,
version 20200217 (http://storage.googleapis.com/books/ngrams/books/20200217/eng/, the 24 files
`1-00000-of-00024.gz` to `1-00023-of-00024.gz`, each held to the SHA-256 the script records), by
Google, licensed under the Creative Commons Attribution 3.0 Unported License
(https://creativecommons.org/licenses/by/3.0/). Only each word's place in that order is kept.
Jean-Baptiste Michel et al., "Quantitative Analysis of Culture Using Millions of Digitized Books",
Science 331 (2011).

## words/syllable 5.0.1

The rules `scaena_core::verse` counts by are written after those of syllable 5.0.1
(https://github.com/words/syllable), and its list of hard words is taken from it:

```
(The MIT License)

Copyright (c) 2014 Titus Wormer <tituswormer@gmail.com>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```
