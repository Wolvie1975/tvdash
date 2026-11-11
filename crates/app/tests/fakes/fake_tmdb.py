#!/usr/bin/env python3
"""Fake TMDB API + image CDN for tvdash's Where to watch test.

Usage: fake_tmdb.py <port-file> <log-file>
The token "good" is accepted; anything else gets 401.
"""
import json, struct, sys, zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

PORT_FILE, LOG_FILE = sys.argv[1], sys.argv[2]

def png(w=4, h=6, rgb=(200, 40, 40)):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

SEARCH = [
    {"id": 603, "media_type": "movie", "title": "The Matrix", "release_date": "1999-03-31",
     "overview": "A hacker learns the truth.", "poster_path": "/matrix.png"},
    {"id": 1, "media_type": "person", "name": "Keanu Reeves"},
    {"id": 1399, "media_type": "tv", "name": "Matrix Tales", "first_air_date": "2011-04-17", "poster_path": None},
    {"id": 604, "media_type": "movie", "title": "The Matrix Reloaded", "release_date": "2003-05-15"},
    {"id": 605, "media_type": "movie", "title": "The Matrix Rare Cut", "release_date": ""},
    {"id": 606, "media_type": "movie", "title": "The Matrix Rental", "release_date": "2021-12-22"},
]
P = lambda i, n, logo=None: {"provider_id": i, "provider_name": n, "logo_path": logo}
OFFERS = {
    "movie/603": {"US": {"link": "https://tmdb.example/603/watch", "flatrate": [P(8, "Netflix")],
                         "rent": [P(2, "Apple TV")]}},
    "tv/1399": {"US": {"flatrate": [P(1899, "Max")]}},
    "movie/604": {"US": {"buy": [P(2, "Apple TV")]}},
    "movie/605": {},
    "movie/606": {"US": {"rent": [P(2, "Apple TV")]}, "GB": {"flatrate": [P(8, "Netflix")]}},
}
PROVIDERS = {
    "movie": [P(8, "Netflix", "/netflix.png"), P(1796, "Netflix Standard with Ads", None), P(2, "Apple TV", "/a.png")],
    "tv": [P(8, "Netflix", "/netflix.png"), P(15, "Hulu", "/hulu.png"), P(1899, "Max", "/max.png")],
}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _send(self, code, body=None, raw=None, ctype="application/json"):
        data = raw if raw is not None else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        with open(LOG_FILE, "a") as f:
            f.write(json.dumps({"path": u.path, "query": q, "auth": self.headers.get("Authorization")}) + "\n")
        p = u.path
        if p.startswith("/t/p/"):
            return self._send(200, raw=png(), ctype="image/png")
        if self.headers.get("Authorization") != "Bearer good":
            return self._send(401, {"status_code": 7, "status_message": "Invalid API key"})
        if p == "/3/configuration":
            return self._send(200, {"images": {}})
        if p == "/3/search/multi":
            return self._send(200, {"page": 1, "results": SEARCH})
        if p.startswith("/3/watch/providers/"):
            return self._send(200, {"results": PROVIDERS[p.rsplit("/", 1)[1]]})
        if p.endswith("/watch/providers"):
            key = p[len("/3/"):-len("/watch/providers")]
            return self._send(200, {"id": 0, "results": OFFERS.get(key, {})})
        self._send(404, {})

srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
with open(PORT_FILE, "w") as f:
    f.write(str(srv.server_address[1]))
srv.serve_forever()
