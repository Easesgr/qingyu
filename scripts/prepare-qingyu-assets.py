#!/usr/bin/env python3
"""Crop and normalize the original AnyiOS image outputs without redrawing them."""
from collections import deque
from pathlib import Path
import hashlib
import json
import runpy
from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parent.parent / "assets/qingyu"
GROUPS = {
    "weather-glyphs": ["sun", "moon", "sun-cloud", "moon-cloud", "cloud", "overcast", "rain", "drizzle",
                       "snow", "thunder", "fog", "wind", "temperature", "humidity", "clock", "bell"],
    "primary-glyphs": ["music", "shelf", "todo", "mail", "quota", "calendar", "weather", "settings",
                       "pages", "placement", "play", "pause", "previous", "next", "volume", "mute"],
    "utility-glyphs": ["close", "left", "right", "plus", "check", "pencil", "refresh", "pin",
                       "clipboard", "search", "folder", "file", "link", "trash", "download", "expand"],
}


def main():
    target = ROOT / "glyphs"
    target.mkdir(parents=True, exist_ok=True)
    manifest = {"source": "AnyiOS image generation", "derivation": "4×4 cell crop, luminance to alpha, optical size normalization", "glyphs": {}}
    for group, names in GROUPS.items():
        source = Image.open(ROOT / "source" / (group + ".png")).convert("RGB")
        for i, name in enumerate(names):
            if name in ("quota", "calendar"):
                continue
            col, row = i % 4, i // 4
            cell = source.crop((round(col * source.width / 4), round(row * source.height / 4),
                                round((col + 1) * source.width / 4), round((row + 1) * source.height / 4)))
            mask = ImageOps.invert(ImageOps.grayscale(cell)).point(lambda v: max(0, min(255, (v - 12) * 255 // 230)))
            bounds = mask.getbbox()
            assert bounds is not None, name
            mask = mask.crop(bounds)
            mask.thumbnail((112, 112), Image.Resampling.LANCZOS)
            glyph = Image.new("RGBA", (128, 128), (255, 255, 255, 0))
            ink = Image.new("RGBA", mask.size, "white")
            ink.putalpha(mask)
            glyph.paste(ink, ((128 - mask.width) // 2, (128 - mask.height) // 2))
            path = target / (name + ".png")
            glyph.save(path, optimize=True)
            manifest["glyphs"][name] = {"source": group, "cell": i, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    icon = Image.open(ROOT / "source/app-icon.png").convert("RGBA")
    pixels = icon.load()
    queue = deque([(0, 0)])
    seen = set()
    while queue:
        x, y = queue.popleft()
        if not (0 <= x < icon.width and 0 <= y < icon.height) or (x, y) in seen:
            continue
        seen.add((x, y))
        red, green, blue, _ = pixels[x, y]
        if min(red, green, blue) < 245:
            continue
        pixels[x, y] = (red, green, blue, 0)
        queue.extend(((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)))
    icon.resize((1024, 1024), Image.Resampling.LANCZOS).save(ROOT / "app-icon.png", optimize=True)
    icon.save(ROOT / "app-icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    # Extract the generated two-plane mark from the final portrait UI reference.
    mark = Image.open(ROOT / "source/portrait-ui.png").convert("RGB").crop((1020, 66, 1077, 117))
    alpha = mark.getchannel("G").point(lambda value: 255 if value > 145 else 0)
    symbol = Image.new("RGBA", mark.size, "white")
    symbol.putalpha(alpha)
    symbol = symbol.crop(symbol.getbbox())
    symbol.thumbnail((36, 30), Image.Resampling.LANCZOS)
    template = Image.new("RGBA", (36, 36), (0, 0, 0, 0))
    template.paste(symbol, ((36 - symbol.width) // 2, (36 - symbol.height) // 2))
    template.save(ROOT / "status-icon.png")
    reference = Image.open(ROOT / "source/portrait-ui.png")
    reference.crop((0, 760, 980, 1000)).resize((1132, 476), Image.Resampling.LANCZOS).save(ROOT / "preview-wallpaper.png")
    reference.crop((1090, 294, 1330, 534)).resize((512, 512), Image.Resampling.LANCZOS).save(ROOT / "music-placeholder.png")
    (ROOT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    # Prefer the current transparent identity when regenerating all UI assets.
    # Historical atlas sources remain reproducible without restoring the old
    # rounded-square logo over the newly approved mark and wordmark.
    if any(all((ROOT / "source" / name).exists() for name in pair) for pair in [
        ["brand-logo-rounded.png", "brand-wordmark-rounded.png"],
        ["brand-logo-transparent.png", "brand-wordmark-transparent.png"],
    ]):
        runpy.run_path(str(Path(__file__).with_name("prepare-qingyu-brand.py")), run_name="__main__")
    print(f"Prepared {len(manifest['glyphs'])} transparent glyphs and Qingyu app/status icons")


if __name__ == "__main__":
    main()
