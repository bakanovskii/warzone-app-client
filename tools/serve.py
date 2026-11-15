#!/usr/bin/env python3
"""Serve the web distribution locally. Phones need HTTPS for PWA and saves."""

import argparse
import ssl
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--bind", default="127.0.0.1")
parser.add_argument("--port", type=int, default=8080)
parser.add_argument("--cert", type=Path)
parser.add_argument("--key", type=Path)
args = parser.parse_args()
if bool(args.cert) != bool(args.key):
    parser.error("--cert and --key must be supplied together")
root = Path(__file__).resolve().parents[1] / "web/dist"
if not (root / "game.wasm").is_file():
    parser.error("First run python3 tools/build.py --web")


class Handler(SimpleHTTPRequestHandler):
    extensions_map = {
        **SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".webmanifest": "application/manifest+json",
    }

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


server = ThreadingHTTPServer((args.bind, args.port), partial(Handler, directory=root))
if args.cert:
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(args.cert, args.key)
    server.socket = context.wrap_socket(server.socket, server_side=True)
print(f"{'https' if args.cert else 'http'}://{args.bind}:{args.port}", flush=True)
server.serve_forever()
