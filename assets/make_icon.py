"""Optional artwork tool: pip install resvg-py pillow; python assets/make_icon.py."""
from io import BytesIO
from pathlib import Path
import struct

from PIL import Image
from resvg_py import svg_to_bytes

root = Path(__file__).resolve().parent
# Exact raster sizes for shell metrics (16/32 DIP) and the 42-DIP dashboard
# logo at common Windows display scales. Render each directly from the SVG.
sizes = sorted({16, 20, 24, 28, 32, 40, 48, 56, 64, 72, 80, 96, 128, 256} |
               {(42 * percent + 50) // 100 for percent in (100,125,150,175,200,225,250,300,350,400)})
entries, images = [], []
offset = 6 + 16 * len(sizes)
for size in sizes:
    svg = root / ('azoth-scope-small.svg' if size <= 24 else 'azoth-scope.svg')
    raw = svg_to_bytes(svg_path=str(svg), width=size*4, height=size*4)
    image = Image.open(BytesIO(raw)).convert('RGBA').resize((size, size), Image.Resampling.LANCZOS)
    png = BytesIO()
    image.save(png, format='PNG')
    data = png.getvalue()
    entries.append(struct.pack('<BBBBHHII', size % 256, size % 256, 0, 0, 1, 32, len(data), offset))
    images.append(data)
    offset += len(data)
    if size == 256:
        (root / 'azoth-scope.png').write_bytes(data)
(root / 'azoth-scope.ico').write_bytes(struct.pack('<HHH', 0, 1, len(sizes)) + b''.join(entries + images))
