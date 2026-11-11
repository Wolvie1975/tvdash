#!/usr/bin/env python3
"""Fake browser for tvdash's end-to-end test.

Logs argv and SIGTERMs as JSON lines to $FAKE_BROWSER_LOG. Forks one helper
process (like Chrome's renderers) so the test can check the whole process group
is closed. FAKE_BROWSER_MODE=exit-now exits at once, like Chrome handing the URL
to an instance that already owns the profile.
"""
import json, os, signal, sys, time

log_path = os.environ["FAKE_BROWSER_LOG"]
def log(obj):
    with open(log_path, "a") as f:
        f.write(json.dumps(obj) + "\n")

if os.environ.get("FAKE_BROWSER_MODE") == "exit-now":
    log({"event": "start", "pid": os.getpid(), "argv": sys.argv[1:], "mode": "exit-now"})
    sys.exit(0)

role = "main"
if os.fork() == 0:
    role = "helper"
else:
    log({"event": "start", "pid": os.getpid(), "pgid": os.getpgrp(), "argv": sys.argv[1:]})

def on_term(*_):
    log({"event": "term", "pid": os.getpid(), "role": role})
    sys.exit(0)

signal.signal(signal.SIGTERM, on_term)
while True:
    time.sleep(0.1)
