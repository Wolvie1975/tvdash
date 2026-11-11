#!/usr/bin/env python3
"""Fake mpv speaking the JSON IPC protocol, for tvdash's end-to-end test.

Reads --input-ipc-server and --start from argv; logs argv and every received
command as JSON lines to $FAKE_MPV_LOG. Advances time-pos by 1 s every 0.1 s.
"""
import json, os, socket, sys, threading, time

log_path = os.environ["FAKE_MPV_LOG"]
def log(obj):
    with open(log_path, "a") as f:
        f.write(json.dumps(obj) + "\n")

args = sys.argv[1:]
log({"argv": args})
sock_path = next(a.split("=", 1)[1] for a in args if a.startswith("--input-ipc-server="))
pos = float(next((a.split("=", 1)[1] for a in args if a.startswith("--start=")), "0"))
paused = False
lock = threading.Lock()

srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
srv.bind(sock_path)
srv.listen(1)
conn, _ = srv.accept()

def send(obj):
    with lock:
        conn.sendall((json.dumps(obj) + "\n").encode())

def ticker():
    global pos
    time.sleep(0.2)
    send({"event": "property-change", "id": 3, "name": "duration", "data": 2700.0})
    send({"event": "property-change", "id": 2, "name": "pause", "data": False})
    while True:
        time.sleep(0.1)
        if not paused:
            pos += 1.0
        try:
            send({"event": "property-change", "id": 1, "name": "time-pos", "data": pos})
        except OSError:
            return

threading.Thread(target=ticker, daemon=True).start()

buf = b""
while True:
    data = conn.recv(4096)
    if not data:
        break
    buf += data
    while b"\n" in buf:
        line, buf = buf.split(b"\n", 1)
        cmd = json.loads(line)["command"]
        log({"command": cmd})
        send({"request_id": 0, "error": "success"})
        if cmd[:2] == ["set_property", "pause"]:
            paused = cmd[2]
            send({"event": "property-change", "id": 2, "name": "pause", "data": paused})
        elif cmd[0] == "seek":
            pos = max(0.0, pos + cmd[1])
        elif cmd[0] == "quit":
            send({"event": "end-file", "reason": "quit"})
            send({"event": "shutdown"})
            conn.close()
            os.unlink(sock_path)
            sys.exit(0)
