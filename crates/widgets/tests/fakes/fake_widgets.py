#!/usr/bin/env python3
"""Fake Open-Meteo for tvdash's widget test.

Usage: fake_widgets.py <port-file> <log-file>
"""
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

PORT_FILE, LOG_FILE = sys.argv[1], sys.argv[2]

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def _send(self, code, body=b"", ctype="application/json"):
        if not isinstance(body, bytes):
            body = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        with open(LOG_FILE, "a") as f:
            f.write(json.dumps({"path": u.path, "query": q}) + "\n")
        p = u.path
        if p == "/v1/forecast":
            return self._send(200, {"current": {"temperature_2m": 48.4, "weather_code": 61},
                                    "daily": {"temperature_2m_max": [55.1], "temperature_2m_min": [38.6]}})
        if p == "/v1/search":
            if q.get("name", [""])[0].lower().startswith("chic"):
                return self._send(200, {"results": [
                    {"name": "Chicago", "latitude": 41.85, "longitude": -87.65, "admin1": "Illinois",
                     "country": "United States", "timezone": "America/Chicago"},
                    {"name": "Chico", "latitude": 39.73, "longitude": -121.84, "admin1": "California",
                     "country": "United States", "timezone": "America/Los_Angeles"},
                ]})
            return self._send(200, {"generationtime_ms": 0.1})
        self._send(404)

srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
with open(PORT_FILE, "w") as f:
    f.write(str(srv.server_address[1]))
srv.serve_forever()
