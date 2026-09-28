#!/usr/bin/env python3
"""Manette virtuelle (uinput) aux identifiants Xbox 360 (045e:028e), pour
tester la chaîne réelle evdev → udev → gilrs → InputMapper → API sans matériel.

usage: virtual_pad.py [attente_initiale_s]
Séquence : RT à fond, stick gauche vers le haut, stick droit à droite, A, B, Y,
puis retour au neutre. Aucune dépendance hors bibliothèque standard.
"""
import fcntl, os, struct, sys, time

UI_SET_EVBIT, UI_SET_KEYBIT, UI_SET_ABSBIT = 0x40045564, 0x40045565, 0x40045567
UI_DEV_CREATE, UI_DEV_DESTROY = 0x5501, 0x5502
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
BTN = dict(A=0x130, B=0x131, X=0x133, Y=0x134, LB=0x136, RB=0x137, BACK=0x13a, START=0x13b, MODE=0x13c, LS=0x13d, RS=0x13e)
ABS = dict(X=0, Y=1, Z=2, RX=3, RY=4, RZ=5, HAT0X=0x10, HAT0Y=0x11)

fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
for ev in (EV_SYN, EV_KEY, EV_ABS):
    fcntl.ioctl(fd, UI_SET_EVBIT, ev)
for code in BTN.values():
    fcntl.ioctl(fd, UI_SET_KEYBIT, code)
absmin, absmax = [0] * 64, [0] * 64
for name, code in ABS.items():
    fcntl.ioctl(fd, UI_SET_ABSBIT, code)
    if name in ("Z", "RZ"):
        absmin[code], absmax[code] = 0, 255
    elif name.startswith("HAT"):
        absmin[code], absmax[code] = -1, 1
    else:
        absmin[code], absmax[code] = -32768, 32767
name = b"Microsoft X-Box 360 pad (virtuel nmeasim)".ljust(80, b"\0")
dev = name + struct.pack("<HHHHi", 0x03, 0x045E, 0x028E, 0x0110, 0)
dev += struct.pack("<64i", *absmax) + struct.pack("<64i", *absmin) + struct.pack("<64i", *([0] * 64)) + struct.pack("<64i", *([0] * 64))
os.write(fd, dev)
fcntl.ioctl(fd, UI_DEV_CREATE)
print("manette virtuelle créée", flush=True)

def emit(t, code, value):
    now = time.time()
    os.write(fd, struct.pack("llHHi", int(now), int((now % 1) * 1e6), t, code, value))
    os.write(fd, struct.pack("llHHi", int(now), int((now % 1) * 1e6), EV_SYN, 0, 0))

def press(b):
    emit(EV_KEY, BTN[b], 1); time.sleep(0.15); emit(EV_KEY, BTN[b], 0)
    print(f"bouton {b}", flush=True)

time.sleep(float(sys.argv[1]) if len(sys.argv) > 1 else 3.0)
press("A"); time.sleep(1.0)                                   # moteurs
emit(EV_ABS, ABS["RZ"], 255); print("RT à fond", flush=True); time.sleep(1.0)
emit(EV_ABS, ABS["RZ"], 0); time.sleep(0.5)
emit(EV_ABS, ABS["Y"], -32768); print("stick gauche haut", flush=True); time.sleep(1.0)
emit(EV_ABS, ABS["RX"], 32767); print("stick droit à droite", flush=True); time.sleep(1.0)
emit(EV_ABS, ABS["RX"], 0); time.sleep(0.5)
press("B"); time.sleep(1.0)                                   # pilote
press("Y"); time.sleep(1.0)                                   # mouillage
emit(EV_ABS, ABS["Y"], 0); time.sleep(1.0)
fcntl.ioctl(fd, UI_DEV_DESTROY)
os.close(fd)
print("manette virtuelle détruite", flush=True)
