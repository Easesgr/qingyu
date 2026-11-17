#!/usr/bin/env python3
"""Prepare the generated transparent Qingyu mark and Chinese wordmark."""
from pathlib import Path
import hashlib
import json
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent / "assets/qingyu"


def cleaned(name: str, alpha_floor: int) -> Image.Image:
    image = Image.open(ROOT / "source" / name).convert("RGBA")
    alpha = image.getchannel("A")
    if alpha.getextrema()[0] != 0:
        raise ValueError(f"{name} must have generated transparency")
    peak = alpha.getextrema()[1]
    alpha = alpha.point(lambda value: max(0, min(255, round((value - alpha_floor) * 255 / (peak - alpha_floor)))))
    image.putalpha(alpha)
    box = alpha.getbbox()
    if box is None:
        raise ValueError(f"{name} has no visible artwork")
    return image.crop(box)


def square(image: Image.Image, size: int, ink_size: int) -> Image.Image:
    image = image.copy()
    image.thumbnail((ink_size, ink_size), Image.Resampling.LANCZOS)
    result = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    result.alpha_composite(image, ((size - image.width) // 2, (size - image.height) // 2))
    return result


def main() -> None:
    logo_source = "brand-logo-rounded.png" if (ROOT / "source/brand-logo-rounded.png").exists() else "brand-logo-transparent.png"
    wordmark_source = "brand-wordmark-rounded.png" if (ROOT / "source/brand-wordmark-rounded.png").exists() else "brand-wordmark-transparent.png"
    # Remove only the generated edge haze; the two-plane shapes stay intact.
    logo = cleaned(logo_source, 16)
    logo.thumbnail((512, 512), Image.Resampling.LANCZOS)
    logo.save(ROOT / "brand-logo.png", optimize=True)
    icon = square(logo, 1024, 844)
    icon.save(ROOT / "app-icon.png", optimize=True)
    icon.save(ROOT / "app-icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])

    # The generated display lettering supplies the silhouette. A template
    # mask lets it follow native foreground colors in light and dark themes.
    wordmark = cleaned(wordmark_source, 128)
    alpha = wordmark.getchannel("A")
    wordmark = Image.new("RGBA", wordmark.size, "white")
    wordmark.putalpha(alpha)
    wordmark.thumbnail((768, 384), Image.Resampling.LANCZOS)
    wordmark.save(ROOT / "brand-wordmark.png", optimize=True)

    template = Image.new("RGBA", logo.size, "white")
    template.putalpha(logo.getchannel("A"))
    square(template, 36, 32).save(ROOT / "status-icon.png", optimize=True)

    manifest_path = ROOT / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    derived = manifest.setdefault("derived_assets", {})
    for name in ["brand-logo.png", "app-icon.png", "app-icon.ico", "status-icon.png"]:
        derived[name] = f"source/{logo_source}; preserve generated alpha, trim faint edge haze, optical margins and resize"
    derived["brand-wordmark.png"] = f"source/{wordmark_source}; generated Chinese lettering alpha, haze removal, monochrome template and resize"
    manifest["brand"] = {
        "provider": "AnyiOS",
        "source_prompts": [f"source/{Path(logo_source).stem}.json", f"source/{Path(wordmark_source).stem}.json"],
        "preparation": "scripts/prepare-qingyu-brand.py",
        "assets": {name: {"sha256": hashlib.sha256((ROOT / name).read_bytes()).hexdigest(),
                          "size": list(Image.open(ROOT / name).size)}
                   for name in ["brand-logo.png", "brand-wordmark.png", "app-icon.png", "status-icon.png"]},
    }
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(manifest["brand"], ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
