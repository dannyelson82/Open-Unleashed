#!/usr/bin/env python3
"""Render the Apple TV brand assets (layered app icons, Top Shelf images) into the app's asset catalog from the
project's own mark (packaging/brand/mark-dark.svg, the variant for dark backgrounds) and procedurally drawn art — no EA / Porsche artwork, no game
screenshots or textures (enhancements-plan.md §2.1). The output is committed, so a build needs none of this.

    python3 -m venv /tmp/brand && /tmp/brand/bin/pip install resvg-py pillow
    /tmp/brand/bin/python platform/tvos/make_brand_assets.py

GPL-3.0-or-later.
"""
import io
import json
import os

import resvg_py
from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
MARK = open(os.path.join(ROOT, "packaging", "brand", "mark-dark.svg")).read()
CAT = os.path.join(HERE, "OpenUnleashed", "Assets.xcassets")
BRAND = os.path.join(CAT, "App Icon & Top Shelf Image.brandassets")
INFO = {"author": "xcode", "version": 1}


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        json.dump(obj, f, indent=2)
        f.write("\n")


def mark(size):
    png = resvg_py.svg_to_bytes(svg_string=MARK, width=size, height=size)
    return Image.open(io.BytesIO(bytes(png))).convert("RGBA")


def back(w, h):
    """Opaque: a dusk sky over dark hills — a vertical gradient with a warm horizon glow."""
    img = Image.new("RGB", (w, h))
    px = img.load()
    hz = int(h * 0.62)
    for y in range(h):
        if y < hz:
            t = y / hz
            c = (int(18 + 120 * t ** 2), int(20 + 50 * t ** 2), int(40 + 20 * t))
        else:
            t = (y - hz) / max(1, h - hz)
            c = (int(22 - 12 * t), int(18 - 8 * t), int(24 - 10 * t))
        for x in range(w):
            px[x, y] = c
    d = ImageDraw.Draw(img)
    # a soft range of hills on the horizon
    pts = [(0, hz)]
    for i in range(0, 21):
        x = w * i / 20
        y = hz - (h * 0.06) * (0.5 + 0.5 * (((i * 37) % 11) / 10.0))
        pts.append((x, y))
    pts += [(w, hz), (w, h), (0, h)]
    d.polygon(pts, fill=(14, 12, 18))
    return img.convert("RGBA")


def road(w, h):
    """Middle layer: a road running to the horizon, with dashed centre line (transparent elsewhere)."""
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    hz = int(h * 0.62)
    cx = w * 0.5
    d.polygon([(cx - w * 0.02, hz), (cx + w * 0.02, hz), (cx + w * 0.42, h), (cx - w * 0.42, h)], fill=(38, 36, 42, 255))
    # edge lines
    d.line([(cx - w * 0.02, hz), (cx - w * 0.42, h)], fill=(190, 180, 160, 200), width=max(2, w // 300))
    d.line([(cx + w * 0.02, hz), (cx + w * 0.42, h)], fill=(190, 180, 160, 200), width=max(2, w // 300))
    # dashes, closer together near the horizon
    n = 9
    for i in range(n):
        t0 = (i / n) ** 1.8
        t1 = ((i + 0.5) / n) ** 1.8
        y0, y1 = hz + (h - hz) * t0, hz + (h - hz) * t1
        wd0, wd1 = 1 + w * 0.004 * t0, 1 + w * 0.004 * t1
        d.polygon([(cx - wd0, y0), (cx + wd0, y0), (cx + wd1, y1), (cx - wd1, y1)], fill=(235, 200, 90, 230))
    return img.filter(ImageFilter.GaussianBlur(radius=max(0.5, w / 1600)))


def front(w, h, scale=0.62):
    """Front layer: the project's mark, centred above the horizon (transparent elsewhere)."""
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    s = int(min(w, h) * scale)
    m = mark(s)
    img.alpha_composite(m, ((w - s) // 2, int(h * 0.42 - s / 2)))
    return img


def imageset(path, files):
    """`files`: list of (filename, scale)."""
    write_json(os.path.join(path, "Contents.json"), {
        "images": [{"idiom": "tv", "filename": f, "scale": sc} for f, sc in files],
        "info": INFO,
    })


def imagestack(name, sizes):
    """A layered icon: Front / Middle / Back, each at the given (w, h, scale) sizes."""
    stack = os.path.join(BRAND, f"{name}.imagestack")
    layers = [("Front", front), ("Middle", road), ("Back", back)]
    for lname, fn in layers:
        lpath = os.path.join(stack, f"{lname}.imagestacklayer")
        files = []
        for w, h, sc in sizes:
            f = f"{lname.lower()}-{w}x{h}.png"
            os.makedirs(os.path.join(lpath, "Content.imageset"), exist_ok=True)
            img = fn(w, h)
            if lname == "Back":
                img = img.convert("RGB")  # the back layer must be opaque
            img.save(os.path.join(lpath, "Content.imageset", f), optimize=True)
            files.append((f, sc))
        imageset(os.path.join(lpath, "Content.imageset"), files)
        write_json(os.path.join(lpath, "Contents.json"), {"info": INFO})
    write_json(os.path.join(stack, "Contents.json"), {"layers": [{"filename": f"{l}.imagestacklayer"} for l, _ in layers], "info": INFO})


def topshelf(name, sizes):
    path = os.path.join(BRAND, f"{name}.imageset")
    os.makedirs(path, exist_ok=True)
    files = []
    for w, h, sc in sizes:
        img = back(w, h)
        img.alpha_composite(road(w, h))
        img.alpha_composite(front(w, h, scale=0.55))
        f = f"topshelf-{w}x{h}.png"
        img.convert("RGB").save(os.path.join(path, f), optimize=True)
        files.append((f, sc))
    imageset(path, files)


def main():
    write_json(os.path.join(CAT, "Contents.json"), {"info": INFO})
    imagestack("App Icon", [(400, 240, "1x"), (800, 480, "2x")])
    imagestack("App Icon - App Store", [(1280, 768, "1x")])
    topshelf("Top Shelf Image", [(1920, 720, "1x"), (3840, 1440, "2x")])
    topshelf("Top Shelf Image Wide", [(2320, 720, "1x"), (4640, 1440, "2x")])
    write_json(os.path.join(BRAND, "Contents.json"), {
        "assets": [
            {"filename": "App Icon - App Store.imagestack", "idiom": "tv", "role": "primary-app-icon", "size": "1280x768"},
            {"filename": "App Icon.imagestack", "idiom": "tv", "role": "primary-app-icon", "size": "400x240"},
            {"filename": "Top Shelf Image Wide.imageset", "idiom": "tv", "role": "top-shelf-image-wide", "size": "2320x720"},
            {"filename": "Top Shelf Image.imageset", "idiom": "tv", "role": "top-shelf-image", "size": "1920x720"},
        ],
        "info": INFO,
    })
    print("brand assets written to", BRAND)


if __name__ == "__main__":
    main()
