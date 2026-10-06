#!/usr/bin/env python3
"""compare.py native.png web.rgba W H [bg] [--mask X,Y,W,H …]: composite the web capture
(premultiplied RGBA8, as a WebGPU canvas with alpha_mode=premultiplied holds
it) over the compositor background the native screenshot shows behind the
window (a grey level, default 63: headless sway's #3f3f3f), and report how far
the two frames are apart, per channel.

The native screenshot must be the window alone at its origin. A cursor sprite
in it is a difference this cannot tell from the renderer's: keep the pointer
off the window, or `--mask` the box the sprite covers (repeatable). Hiding
the cursor in the compositor is no answer — sway's `hide_cursor` clears the
pointer focus, so the app sees a leave and drops its hover. Needs
ImageMagick's `convert`."""
import subprocess, sys
args = sys.argv[1:]
masks = []
while '--mask' in args:
    i = args.index('--mask')
    masks.append(tuple(int(v) for v in args[i + 1].split(',')))
    del args[i:i + 2]
native_png, web_path, W, H = args[0], args[1], int(args[2]), int(args[3])
bg = int(args[4]) if len(args) > 4 else 63
masked = lambda x, y: any(mx <= x < mx + mw and my <= y < my + mh for mx, my, mw, mh in masks)
native = subprocess.run(['convert', native_png, '-alpha', 'off', 'rgb:-'], capture_output=True, check=True).stdout
web = open(web_path, 'rb').read()
assert len(native) == W * H * 3 and len(web) == W * H * 4, (len(native), len(web))
hist = {}
worst = (0, None)
for i in range(W * H):
    if masks and masked(i % W, i // W):
        continue
    a = web[4 * i + 3]
    for c in range(3):
        w = web[4 * i + c] + bg * (255 - a) / 255.0
        d = abs(round(w) - native[3 * i + c])
        hist[d] = hist.get(d, 0) + 1
        if d > worst[0]:
            worst = (d, (i % W, i // W))
px_over = lambda t: sum(n for d, n in hist.items() if d > t)
print(f"channels compared: {sum(hist.values())}")
print("difference histogram (levels of 255):", dict(sorted(hist.items())))
print(f"channels off by >1: {px_over(1)}, >2: {px_over(2)}, >4: {px_over(4)}, >8: {px_over(8)}")
print(f"worst: {worst[0]} at {worst[1]}")
