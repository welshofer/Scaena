# The single-file export's player

`standalone.html` here is the page `scaena export --format html` fills in with a bundle
(PLAN 2.5, SPEC §10): the web player, its code, and the engine's player module, gzipped, in
one file. `just web` builds it from `web/standalone.html` (`web/vite.standalone.config.ts`);
it is not committed.

`scaena-export` carries it when it is here as the crate builds: its build script watches this
directory. Built without it, `export --format html` exits 3 and says to run `just web`, then
build `scaena` again.
