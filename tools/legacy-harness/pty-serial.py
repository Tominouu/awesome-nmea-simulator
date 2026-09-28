#!/usr/bin/env python3
"""Paire de PTY virtuels pour instruire le transport serie du legacy.

usage: pty-serial.py <fichier_chemin> <sortie.log> <duree_s> [inject_at_s]

Cree un PTY, ecrit le chemin de l'esclave dans <fichier_chemin> (a passer en
server.serial.port), puis lit le maitre pendant <duree_s> secondes. Chaque
lecture est journalisee avec son horodatage relatif, comme cap-tcp.js. A
<inject_at_s>, une phrase est ecrite vers l'application pour observer le
traitement de l'entree serie.
"""
import os, sys, time, tty, select, json

path_file, out, dur = sys.argv[1], sys.argv[2], float(sys.argv[3])
inject_at = float(sys.argv[4]) if len(sys.argv) > 4 else None

master, slave = os.openpty()
tty.setraw(slave)
name = os.ttyname(slave)
with open(path_file, "w") as f:
    f.write(name)
print(f"pty esclave {name}", flush=True)

t0 = None
start = time.monotonic()
injected = False
total = 0
with open(out, "w") as log:
    while time.monotonic() - start < dur:
        r, _, _ = select.select([master], [], [], 0.2)
        if r:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            now = time.monotonic()
            if t0 is None:
                t0 = now
            total += len(data)
            log.write(f"[{int((now - t0) * 1000)}ms] {json.dumps(data.decode('latin-1'))}\n")
            log.flush()
        if inject_at is not None and not injected and t0 is not None and time.monotonic() - t0 >= inject_at:
            os.write(master, b"$GPTXT,01,01,02,SERIAL INPUT PROBE*00\r\n")
            injected = True
            log.write(f"[{int((time.monotonic() - t0) * 1000)}ms] INJECT $GPTXT,01,01,02,SERIAL INPUT PROBE*00\n")
print(f"octets recus {total}", flush=True)
