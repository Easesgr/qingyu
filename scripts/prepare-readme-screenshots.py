#!/usr/bin/env python3
"""Crop native window captures and record their provenance. Requires Pillow."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re

from PIL import Image

SCENES = (
    "overview", "music", "playlists", "shelf", "clipboard", "library",
    "library-detail", "todos", "mail", "mail-reader", "weather", "sunlight",
    "agents", "mac-stats", "notifications", "notice", "collapsed", "top-overview",
    "settings", "modules", "pets", "completion", "side-overview", "style-classic",
    "style-cream", "style-fluid", "style-glass-light", "style-glass-dark",
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture_dir", type=Path, help="PNG window captures, named by scene")
    parser.add_argument("--output", type=Path, default=Path("docs/images"))
    args = parser.parse_args()
    missing = [name for name in SCENES if not (args.capture_dir / f"{name}.png").is_file()]
    if missing:
        parser.error("Missing native captures: " + ", ".join(missing))
    args.output.mkdir(parents=True, exist_ok=True)
    captures = []
    for name in SCENES:
        source = args.capture_dir / f"{name}.png"
        with Image.open(source) as original:
            if name in {"settings", "modules", "pets"}:
                crop = (0, 0, *original.size)
            else:
                if original.size != (2000, 1696):
                    parser.error(f"{name}: expected 2× 1000×820 window plus title bar, got {original.size}; adjust crop coordinates for this capture environment")
                crop = {
                    "side-overview": (480, 190, 1300, 1515),
                    "collapsed": (500, 390, 1500, 540),
                    "notice": (500, 390, 1500, 650),
                    "completion": (500, 390, 1500, 650),
                }.get(name, (500, 390, 1500, 1120))
            cropped = original.crop(crop).convert("RGB")
            # Fresh pixel image strips incidental screenshot metadata.
            image = Image.frombytes("RGB", cropped.size, cropped.tobytes())
            destination = args.output / source.name
            image.save(destination, optimize=True)
            captures.append({
                "file": source.name,
                "native_window_size": list(original.size),
                "crop": list(crop),
                "size": list(image.size),
                "native_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
                "bytes": destination.stat().st_size,
            })
    repo = Path(__file__).resolve().parent.parent
    version = re.search(r'^version = "([^"]+)"', (repo / "Cargo.toml").read_text(), re.M).group(1)
    manifest = {
        "project": "Qingyu", "version": version,
        "prepared_at": datetime.now(timezone.utc).isoformat(),
        "capture_method": "macOS native window screenshots captured through Computer Use",
        "renderer": "production App rendered by egui/eframe in a native debug showcase window",
        "data": "isolated examples; no real mail, conversations, credentials, or audio playback",
        "interface_source": "src/app.rs",
        "showcase_source": "src/showcase.rs",
        "postprocessing": "rectangular crop, metadata removal and lossless PNG compression only",
        "captures": captures,
    }
    (args.output / "capture-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    total = sum(item["bytes"] for item in captures)
    landscape = sum(item["size"][0] > item["size"][1] for item in captures)
    print(f"Prepared {len(captures)} screenshots: {landscape} landscape, {len(captures)-landscape} portrait; {total/1024/1024:.2f} MiB")


if __name__ == "__main__":
    main()
