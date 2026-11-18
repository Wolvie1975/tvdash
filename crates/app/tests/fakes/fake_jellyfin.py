#!/usr/bin/env python3
"""Minimal fake Jellyfin for tvdash's end-to-end test.

Usage: fake_jellyfin.py <port-file> <log-file>
Writes the chosen port to <port-file>; appends one JSON line per request to <log-file>.
Pretends to be pre-10.9: /UserItems/Resume returns 404 so the client must fall back.
"""
import json, struct, sys, zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

PORT_FILE, LOG_FILE = sys.argv[1], sys.argv[2]
polls = {"n": 0}

def png(w=4, h=6, rgb=(91, 157, 255)):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

ITEMS = [
    {"Id": "ep1", "Name": "The Pilot", "Type": "Episode", "SeriesName": "Some Show", "SeriesId": "ser1",
     "SeriesPrimaryImageTag": "t", "ParentIndexNumber": 1, "IndexNumber": 1, "RunTimeTicks": 27_000_000_000,
     "UserData": {"PlaybackPositionTicks": 6_000_000_000}},
    {"Id": "mov1", "Name": "A Film", "Type": "Movie", "ProductionYear": 1999, "RunTimeTicks": 72_000_000_000,
     "ImageTags": {"Primary": "p"}, "UserData": {"PlaybackPositionTicks": 0}},
]

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _log(self, body=None):
        u = urlparse(self.path)
        with open(LOG_FILE, "a") as f:
            f.write(json.dumps({"method": self.command, "path": u.path, "query": parse_qs(u.query),
                                "auth": self.headers.get("Authorization", ""), "body": body}) + "\n")

    def _send(self, code, obj=None, raw=None, ctype="application/json"):
        data = raw if raw is not None else (json.dumps(obj).encode() if obj is not None else b"")
        self.send_response(code)
        if data:
            self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _authed(self):
        return 'Token="tok"' in self.headers.get("Authorization", "")

    def do_GET(self):
        self._log()
        p = urlparse(self.path).path
        if p == "/QuickConnect/Connect":
            polls["n"] += 1
            return self._send(200, {"Authenticated": polls["n"] >= 2, "Secret": "s3cr3t", "Code": "424242"})
        if not self._authed():
            return self._send(401)
        if p == "/UserItems/Resume":
            return self._send(404)
        if p == "/Users/u1/Items/Resume":
            return self._send(200, {"Items": ITEMS, "TotalRecordCount": len(ITEMS)})
        if p == "/Items/Latest":
            return self._send(200, [{"Id": "new1", "Name": "New Film", "Type": "Movie", "ImageTags": {"Primary": "n"}}])
        if p == "/Items" and "ProviderIds" in parse_qs(urlparse(self.path).query).get("fields", [""])[0]:
            # Library listing with TMDB IDs (Where to watch matches).
            return self._send(200, {"Items": [
                {"Id": "m604", "Name": "The Matrix Reloaded", "Type": "Movie", "ProductionYear": 2003,
                 "ProviderIds": {"Tmdb": "604", "Imdb": "tt0234215"}},
                {"Id": "s1", "Name": "Some Show", "Type": "Series", "ProviderIds": {"Tmdb": "999"}},
                {"Id": "nope", "Name": "No IDs", "Type": "Movie"},
            ]})
        if p == "/Items":
            term = parse_qs(urlparse(self.path).query).get("searchTerm", [""])[0].lower()
            found = [i for i in ITEMS if term in i["Name"].lower()]
            return self._send(200, {"Items": found, "TotalRecordCount": len(found)})
        if p.startswith("/Items/") and p.endswith("/Images/Primary"):
            return self._send(200, raw=png(), ctype="image/png")
        self._send(404)

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        body = json.loads(self.rfile.read(n) or b"null")
        self._log(body)
        p = urlparse(self.path).path
        if p == "/QuickConnect/Initiate":
            return self._send(200, {"Secret": "s3cr3t", "Code": "424242", "Authenticated": False})
        if p == "/Users/AuthenticateByName":
            ok = body == {"Username": "me", "Pw": "pw"}
            return self._send(200 if ok else 401, {"AccessToken": "tok", "User": {"Id": "u1"}} if ok else None)
        if p == "/Users/AuthenticateWithQuickConnect":
            ok = body == {"Secret": "s3cr3t"}
            return self._send(200 if ok else 400, {"AccessToken": "tok", "User": {"Id": "u1"}} if ok else None)
        if p.startswith("/Sessions/Playing"):
            return self._send(204 if self._authed() else 401)
        self._send(404)

srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
with open(PORT_FILE, "w") as f:
    f.write(str(srv.server_address[1]))
srv.serve_forever()
