#!/usr/bin/env python3
"""Gera as imagens do assistente de instalação do Windows a partir da marca.

Lê os SVG de studio/public/brand (os fios da marca e o nome "Lace") e grava
em installer/windows/art/ os PNG que o lace.iss usa:

  wizard-164.png, wizard-328.png   a faixa da esquerda das páginas de
                                   boas-vindas e de fim (100% e 200%), na
                                   proporção do WizardSizePercent do lace.iss
  small-55.png, small-110.png      o ícone do canto das outras páginas

Só a biblioteca padrão: o desenho é feito aqui (traços com ponta redonda e
preenchimento com a regra nonzero, com antisserrilhado por amostragem) e o
PNG é escrito à mão. Rode de novo quando a marca mudar:

  python scripts/installer_art.py
"""

import math
import re
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "studio" / "public" / "brand"
OUT = ROOT / "installer" / "windows" / "art"

NAVY = (0x2E, 0x43, 0x74)
NAVY_DEEP = (0x1D, 0x2B, 0x4D)
CREAM = (0xF4, 0xEF, 0xE6)
SKY = (0x9B, 0xB0, 0xE8)

# O WizardSizePercent do lace.iss (largura, altura): a faixa da esquerda,
# 164 por 314 no tamanho 100,100, cresce nessa proporção.
SIZE_PERCENT = (130, 150)


# ------------------------------------------------------------- SVG -----

def tokens(d):
    return re.findall(r"[MLHVQZ]|-?\d*\.?\d+(?:e-?\d+)?", d)


def flatten(d):
    """Os contornos de um caminho só com M, L, H, V, Q e Z, em polígonos."""
    shapes, current = [], []
    x = y = 0.0
    it = iter(tokens(d))
    cmd = None
    for tok in it:
        if tok in "MLHVQZ":
            cmd = tok
            if cmd == "Z":
                if current:
                    shapes.append(current)
                current = []
                continue
            tok = next(it)
        if cmd == "M":
            if current:
                shapes.append(current)
            x, y = float(tok), float(next(it))
            current = [(x, y)]
            cmd = "L"
        elif cmd == "L":
            x, y = float(tok), float(next(it))
            current.append((x, y))
        elif cmd == "H":
            x = float(tok)
            current.append((x, y))
        elif cmd == "V":
            y = float(tok)
            current.append((x, y))
        elif cmd == "Q":
            cx, cy = float(tok), float(next(it))
            ex, ey = float(next(it)), float(next(it))
            for i in range(1, 9):
                t = i / 8
                px = (1 - t) ** 2 * x + 2 * (1 - t) * t * cx + t * t * ex
                py = (1 - t) ** 2 * y + 2 * (1 - t) * t * cy + t * t * ey
                current.append((px, py))
            x, y = ex, ey
    if current:
        shapes.append(current)
    return shapes


def brand_paths():
    """Os fios da marca (polilinhas) e o nome "Lace" (polígonos), no quadro
    do lockup (206 por 64)."""
    svg = (BRAND / "lace-lockup.svg").read_text(encoding="utf-8")
    stroke_group = re.search(r'<g fill="none" stroke="[^"]+"[^>]*>(.*?)</g>', svg, re.S).group(1)
    wires = [flatten(d)[0] for d in re.findall(r' d="([^"]+)"', stroke_group)]
    rest = svg.replace(stroke_group, "")
    word = []
    for d in re.findall(r'<path fill="[^"]+" d="([^"]+)"', rest):
        word.extend(flatten(d))
    return wires, word


# ---------------------------------------------------------- desenho -----

class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [[(0, 0, 0, 0)] * w for _ in range(h)]

    def fill_rect(self, color, alpha=1.0):
        for y in range(self.h):
            for x in range(self.w):
                self.blend(x, y, color, alpha)

    def gradient(self, top, bottom):
        for y in range(self.h):
            t = y / max(1, self.h - 1)
            c = tuple(round(a + (b - a) * t) for a, b in zip(top, bottom))
            for x in range(self.w):
                self.px[y][x] = (*c, 255)

    def blend(self, x, y, color, alpha):
        if alpha <= 0 or not (0 <= x < self.w and 0 <= y < self.h):
            return
        r, g, b, a = self.px[y][x]
        a0 = a / 255
        out_a = alpha + a0 * (1 - alpha)
        if out_a == 0:
            return
        mix = [
            (c * alpha + o * a0 * (1 - alpha)) / out_a
            for c, o in zip(color, (r, g, b))
        ]
        self.px[y][x] = (*[round(v) for v in mix], round(out_a * 255))

    def stroke(self, points, width, color, alpha=1.0):
        """Polilinha com ponta e junção redondas."""
        r = width / 2
        cover = {}
        for (x0, y0), (x1, y1) in zip(points, points[1:]):
            minx = int(math.floor(min(x0, x1) - r - 1))
            maxx = int(math.ceil(max(x0, x1) + r + 1))
            miny = int(math.floor(min(y0, y1) - r - 1))
            maxy = int(math.ceil(max(y0, y1) + r + 1))
            dx, dy = x1 - x0, y1 - y0
            ll = dx * dx + dy * dy
            for py in range(max(0, miny), min(self.h, maxy + 1)):
                for px in range(max(0, minx), min(self.w, maxx + 1)):
                    cx, cy = px + 0.5, py + 0.5
                    t = 0.0 if ll == 0 else max(0.0, min(1.0, ((cx - x0) * dx + (cy - y0) * dy) / ll))
                    d = math.hypot(cx - (x0 + t * dx), cy - (y0 + t * dy))
                    c = max(0.0, min(1.0, r + 0.5 - d))
                    if c > cover.get((px, py), 0):
                        cover[(px, py)] = c
        for (px, py), c in cover.items():
            self.blend(px, py, color, c * alpha)

    def fill(self, polygons, color, alpha=1.0, samples=4):
        """Preenchimento nonzero, com `samples` x `samples` amostras por pixel."""
        edges = []
        for poly in polygons:
            for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
                if y0 != y1:
                    edges.append((x0, y0, x1, y1))
        if not edges:
            return
        ys = [e[1] for e in edges] + [e[3] for e in edges]
        top, bottom = max(0, int(min(ys))), min(self.h - 1, int(max(ys)) + 1)
        for py in range(top, bottom + 1):
            row = [0.0] * self.w
            for s in range(samples):
                sy = py + (s + 0.5) / samples
                hits = []
                for x0, y0, x1, y1 in edges:
                    if (y0 <= sy < y1) or (y1 <= sy < y0):
                        hits.append((x0 + (sy - y0) * (x1 - x0) / (y1 - y0), 1 if y1 > y0 else -1))
                hits.sort()
                wind = 0
                for (xa, da), (xb, _) in zip(hits, hits[1:]):
                    wind += da
                    if wind != 0:
                        for sx in range(int(xa * samples), int(xb * samples)):
                            px = sx // samples
                            if 0 <= px < self.w:
                                row[px] += 1 / (samples * samples)
            for px, c in enumerate(row):
                if c > 0:
                    self.blend(px, py, color, min(1.0, c) * alpha)

    def round_mask(self, radius):
        """Cantos arredondados: transparente fora do retângulo de cantos `radius`."""
        for y in range(self.h):
            for x in range(self.w):
                cx = min(max(x + 0.5, radius), self.w - radius)
                cy = min(max(y + 0.5, radius), self.h - radius)
                d = math.hypot(x + 0.5 - cx, y + 0.5 - cy)
                keep = max(0.0, min(1.0, radius + 0.5 - d))
                r, g, b, a = self.px[y][x]
                self.px[y][x] = (r, g, b, round(a * keep))

    def save(self, path):
        raw = b"".join(
            b"\x00" + bytes(v for p in row for v in p) for row in self.px
        )

        def chunk(kind, data):
            body = kind + data
            return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

        png = b"\x89PNG\r\n\x1a\n"
        png += chunk(b"IHDR", struct.pack(">IIBBBBB", self.w, self.h, 8, 6, 0, 0, 0))
        png += chunk(b"IDAT", zlib.compress(raw, 9))
        png += chunk(b"IEND", b"")
        path.write_bytes(png)


def place(points, scale, dx, dy):
    return [(x * scale + dx, y * scale + dy) for x, y in points]


# ---------------------------------------------------------- imagens -----

MARK_BOX = (4, 19, 60, 45)  # onde os fios ficam no quadro do lockup


def wizard(w, h):
    """A faixa da esquerda: fundo azul da marca, os fios em creme, o nome
    embaixo deles e, no pé, uma barra de fios cruzados em azul claro."""
    wires, word = brand_paths()
    k = w / 164
    c = Canvas(w, h)
    c.gradient(NAVY, NAVY_DEEP)

    # Os fios, grandes, um pouco acima do meio da faixa.
    mark_w = 120 * k
    s = mark_w / (MARK_BOX[2] - MARK_BOX[0])
    dx = (w - mark_w) / 2 - MARK_BOX[0] * s
    top = h * 0.24
    dy = top - MARK_BOX[1] * s
    for wire in wires:
        c.stroke(place(wire, s, dx, dy), 6 * s, CREAM)

    # O nome, centrado embaixo.
    xs = [x for poly in word for x, _ in poly]
    ys = [y for poly in word for _, y in poly]
    word_w = 84 * k
    sw = word_w / (max(xs) - min(xs))
    wdx = (w - word_w) / 2 - min(xs) * sw
    wdy = top + 72 * k - min(ys) * sw
    c.fill([place(p, sw, wdx, wdy) for p in word], CREAM)

    # No pé, os fios repetidos lado a lado, como uma renda.
    band_s = 20 * k / (MARK_BOX[2] - MARK_BOX[0])
    unit = (MARK_BOX[2] - MARK_BOX[0]) * band_s
    for row, alpha in ((0, 0.34), (1, 0.18), (2, 0.09)):
        y = h - (58 - row * 17) * k - MARK_BOX[1] * band_s
        x = -MARK_BOX[0] * band_s - (unit / 2 if row % 2 else 0)
        while x < w:
            for wire in wires:
                c.stroke(place(wire, band_s, x, y), 6 * band_s, SKY, alpha)
            x += unit
    return c


def small(size):
    """O ícone do aplicativo: quadrado azul de cantos redondos com os fios."""
    wires, _ = brand_paths()
    c = Canvas(size, size)
    c.fill_rect(NAVY)
    c.round_mask(size * 14 / 64)
    s = size * 0.78 / 64
    off = size * 0.11
    for wire in wires:
        c.stroke(place(wire, s, off, off), 6 * s, CREAM)
    return c


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for w in (164, 328):
        stretch = SIZE_PERCENT[1] / SIZE_PERCENT[0]
        wizard(w, round(w * 314 * stretch / 164)).save(OUT / f"wizard-{w}.png")
    for size in (55, 110):
        small(size).save(OUT / f"small-{size}.png")
    for p in sorted(OUT.glob("*.png")):
        print(f"{p.relative_to(ROOT)}  {p.stat().st_size} bytes")


if __name__ == "__main__":
    main()
