#!/usr/bin/env python3
"""Minimal fake plex.tv + Plex Media Server (same port) for tvdash's end-to-end test.

Usage: fake_plex.py <port-file> <log-file>
The resources list offers an unreachable local address first, so discovery must
skip it. /status/sessions includes one session from the caller's own client id,
which the client must filter out.
"""
import json, struct, sys, zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

PORT_FILE, LOG_FILE = sys.argv[1], sys.argv[2]
state = {"polls": 0, "port": 0}
TOKEN = "ptok"

def png(w=4, h=6, rgb=(229, 168, 75)):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

ON_DECK = [
    {"ratingKey": "101", "key": "/library/metadata/101", "type": "episode", "title": "Chapter One",
     "grandparentTitle": "Plex Show", "parentIndex": 3, "index": 2, "duration": 3_000_000, "viewOffset": 1_200_000,
     "thumb": "/library/metadata/101/thumb/1", "grandparentThumb": "/library/metadata/100/thumb/1"},
    {"ratingKey": "200", "key": "/library/metadata/200", "type": "movie", "title": "Plex Film", "year": 2001,
     "duration": 6_000_000, "thumb": "/library/metadata/200/thumb/1"},
]

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _send(self, code, obj=None, raw=None, ctype="application/json"):
        data = raw if raw is not None else (json.dumps(obj).encode() if obj is not None else b"")
        self.send_response(code)
        if data:
            self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _route(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        token = self.headers.get("X-Plex-Token") or q.get("X-Plex-Token", [""])[0]
        client = self.headers.get("X-Plex-Client-Identifier", "")
        with open(LOG_FILE, "a") as f:
            f.write(json.dumps({"method": self.command, "path": u.path, "query": q, "token": token,
                                "client": client, "product": self.headers.get("X-Plex-Product", "")}) + "\n")
        p = u.path

        # plex.tv
        if p == "/api/v2/pins" and self.command == "POST":
            return self._send(201, {"id": 77, "code": "WXYZ", "authToken": None, "expiresIn": 900})
        if p == "/api/v2/pins/77":
            state["polls"] += 1
            return self._send(200, {"id": 77, "code": "WXYZ", "authToken": TOKEN if state["polls"] >= 2 else None})
        if token != TOKEN:
            return self._send(401)
        if p == "/api/v2/resources":
            return self._send(200, [
                {"name": "Some Player", "provides": "player", "owned": True, "connections": []},
                {"name": "Republic", "provides": "server", "owned": True, "accessToken": TOKEN, "connections": [
                    {"uri": "http://127.0.0.1:1", "local": True, "relay": False},
                    {"uri": f"http://127.0.0.1:{state['port']}", "local": True, "relay": False},
                    {"uri": "https://relay.invalid", "local": False, "relay": True},
                ]},
            ])

        # Plex Media Server
        if p == "/identity":
            return self._send(200, {"MediaContainer": {"machineIdentifier": "srv"}})
        if p == "/":
            return self._send(200, {"MediaContainer": {
                "friendlyName": "Republic", "version": "1.40.0", "myPlexSubscription": True}})
        if p == "/statistics/resources":
            return self._send(200, {"MediaContainer": {"StatisticsResources": [
                {"at": 1000, "hostCpuUtilization": 40, "processCpuUtilization": 20,
                 "hostMemoryUtilization": 50, "processMemoryUtilization": 30}]}})
        if p == "/statistics/bandwidth":
            return self._send(200, {"MediaContainer": {"StatisticsBandwidth": [
                {"at": 1000, "lan": True, "bytes": 6_000_000},
                {"at": 1000, "lan": False, "bytes": 3_000_000}]}})
        if p == "/activities":
            return self._send(200, {"MediaContainer": {"Activity": [
                {"uuid": "scan-1", "type": "library.refresh", "title": "Scanning Movies",
                 "subtitle": "Movies", "progress": 72, "cancellable": True}]}})
        if p == "/library/sections":
            return self._send(200, {"MediaContainer": {"Directory": [
                {"key": "1", "title": "Movies", "type": "movie", "scannedAt": 900, "refreshing": False}]}})
        if p == "/library/sections/1/all":
            return self._send(200, {"MediaContainer": {"totalSize": 42}})
        if p in ("/library/sections/1/refresh", "/library/sections/all/refresh"):
            return self._send(200)
        if p == "/status/sessions/terminate":
            return self._send(200)
        if p.startswith("/library/metadata/") and p.endswith("/posters"):
            return self._send(200, {"MediaContainer": {"Metadata": [
                {"key": "/art/poster/1", "thumb": "/art/poster/1", "selected": True, "provider": "tmdb"},
                {"key": "/art/poster/2", "thumb": "/art/poster/2", "selected": False, "provider": "local"}]}})
        if p.startswith("/library/metadata/") and p.endswith("/arts"):
            return self._send(200, {"MediaContainer": {"Metadata": [
                {"key": "/art/backdrop/1", "thumb": "/art/backdrop/1", "selected": True, "provider": "tmdb"}]}})
        if p.startswith("/library/metadata/") and p.endswith("/refresh"):
            return self._send(200)
        if p.startswith("/library/metadata/") and p.endswith(("/poster", "/art")):
            return self._send(200)
        if p.startswith("/art/"):
            return self._send(200, raw=png(), ctype="image/png")
        if p == "/library/recentlyAdded":
            return self._send(200, {"MediaContainer": {"Metadata": [
                {"ratingKey": "300", "type": "movie", "title": "Fresh Plex Film", "year": 2026, "thumb": "/t/300"}]}})
        if p == "/hubs/search":
            return self._send(200, {"MediaContainer": {"Hub": [
                {"type": "movie", "Metadata": [{"ratingKey": "200", "type": "movie", "title": "Plex Film"}]},
                {"type": "artist", "Metadata": [{"ratingKey": "900", "type": "artist", "title": "A Band"}]}]}})
        if p == "/library/onDeck":
            return self._send(200, {"MediaContainer": {"size": len(ON_DECK), "Metadata": ON_DECK}})
        if p.startswith("/library/metadata/"):
            rk = p.rsplit("/", 1)[1]
            return self._send(200, {"MediaContainer": {"Metadata": [{"ratingKey": rk, "type": "episode", "title": "x",
                "thumb": f"/library/metadata/{rk}/thumb/1",
                "Media": [{"Part": [{"key": f"/library/parts/9{rk}/file.mkv"}]}]}]}})
        if p == "/photo/:/transcode":
            return self._send(200, raw=png(), ctype="image/png")
        if p == "/status/sessions":
            return self._send(200, {"MediaContainer": {"size": 2, "Metadata": [
                {"ratingKey": "200", "type": "movie", "title": "Plex Film", "duration": 6_000_000, "viewOffset": 60_000,
                 "sessionKey": 200,
                 "Player": {"title": "Bedroom TV", "state": "paused", "machineIdentifier": "bedroom"}},
                {"ratingKey": "101", "type": "episode", "title": "Chapter One", "grandparentTitle": "Plex Show",
                 "Player": {"title": "tv-laptop", "state": "playing", "machineIdentifier": client}},
            ]}})
        if p == "/:/timeline":
            return self._send(200)
        self._send(404)

    do_GET = _route
    do_POST = _route
    do_PUT = _route
    do_DELETE = _route

srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
state["port"] = srv.server_address[1]
with open(PORT_FILE, "w") as f:
    f.write(str(state["port"]))
srv.serve_forever()
