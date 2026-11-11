#!/usr/bin/env python3
"""Fake Open-Meteo + calendar feed + Portainer + Uptime Kuma for tvdash's widget test.

Usage: fake_widgets.py <port-file> <log-file>
/cal.ics contains a timed event starting one hour from now (so it's always "next").
/revoked.ics returns 404, like a reset Google secret address.
"""
import json, sys
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

PORT_FILE, LOG_FILE = sys.argv[1], sys.argv[2]

def ics():
    start = datetime.now(timezone.utc).replace(microsecond=0) + timedelta(hours=1)
    end = start + timedelta(minutes=30)
    f = lambda t: t.strftime("%Y%m%dT%H%M%SZ")
    return ("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:soon@fake\r\n"
            f"DTSTART:{f(start)}\r\nDTEND:{f(end)}\r\nSUMMARY:Soon\\, really\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")

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
            f.write(json.dumps({"path": u.path, "query": q, "api_key": self.headers.get("X-API-Key")}) + "\n")
        p = u.path
        if p == "/v1/forecast":
            return self._send(200, {"current": {"temperature_2m": 48.4, "weather_code": 61},
                                    "daily": {"temperature_2m_max": [55.1], "temperature_2m_min": [38.6]}})
        if p == "/cal.ics":
            return self._send(200, ics().encode(), "text/calendar")
        if p == "/revoked.ics":
            return self._send(404, b"Not Found", "text/plain")
        if p == "/api/endpoints/2/docker/containers/json":
            if self.headers.get("X-API-Key") != "pk":
                return self._send(401, {"message": "Unauthorized"})
            return self._send(200, [{"State": "running"}, {"State": "running"}, {"State": "exited"}])
        if p == "/api/status-page/homelab":
            return self._send(200, {"publicGroupList": [{"monitorList": [
                {"id": 1, "name": "Plex"}, {"id": 2, "name": "NAS"}, {"id": 3, "name": "Router"}]}]})
        if p == "/api/status-page/heartbeat/homelab":
            return self._send(200, {"heartbeatList": {"1": [{"status": 1}], "2": [{"status": 1}, {"status": 0}],
                                                      "3": [{"status": 1}]}})
        self._send(404)

srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
with open(PORT_FILE, "w") as f:
    f.write(str(srv.server_address[1]))
srv.serve_forever()
