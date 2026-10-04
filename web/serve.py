#!/usr/bin/env python3
"""Serve the site this file stands in on this machine only (PLAN 2.7). `just site` copies it into
target/site, beside the pages and the demo deck.

    python3 serve.py            then open http://localhost:8080/  (the player)
                                       or http://localhost:8080/editor.html
    python3 serve.py 9000       on another port

It listens on 127.0.0.1, so nothing else on the network can reach it, and it serves .wasm as
application/wasm, which older Pythons' http.server does not. Ctrl-C stops it.
"""
import http.server
import os
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8080


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".json": "application/json",
        ".scn": "text/plain; charset=utf-8",
    }

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=ROOT, **kwargs)


if __name__ == "__main__":
    with http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler) as server:
        print(f"Scaena at http://localhost:{PORT}/ (player) and http://localhost:{PORT}/editor.html (editor). Ctrl-C stops it.")
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
