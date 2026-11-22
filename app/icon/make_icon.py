"""Draws the Side Chess icon (original artwork, GPL-3.0-or-later with the rest of the repo):
a chess pawn with a spark and a few circuit nodes on a blue board corner.
Run from the repo root:  python app/icon/make_icon.py   (needs Pillow)
Writes app/icon/icon.png (256), icon-128.png, icon.ico and icon.icns."""
from PIL import Image, ImageDraw, ImageFilter
import math, os

S = 2048  # draw big, shrink for smooth edges
out = os.path.dirname(os.path.abspath(__file__))

def lerp(a, b, t): return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))

# background: rounded square, deep blue gradient
bg = Image.new("RGBA", (S, S), (0, 0, 0, 0))
grad = Image.new("RGBA", (S, S))
gp = grad.load()
top, bot = (17, 82, 212), (8, 36, 110)
for y in range(S):
    c = lerp(top, bot, y / S)
    for x in range(S):
        gp[x, y] = (*c, 255)
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, S - 1, S - 1), radius=int(S * 0.225), fill=255)
bg.paste(grad, (0, 0), mask)

# faint checkerboard across the lower half
board = Image.new("RGBA", (S, S), (0, 0, 0, 0))
bd = ImageDraw.Draw(board)
n = 6; cell = S // n
for r in range(n):
    for c in range(n):
        if (r + c) % 2 == 0 and r >= 2:
            bd.rectangle((c * cell, r * cell, (c + 1) * cell, (r + 1) * cell), fill=(255, 255, 255, 22))
bg.alpha_composite(Image.composite(board, Image.new("RGBA", (S, S), (0, 0, 0, 0)), mask))

# circuit lines and nodes (the "AI" part), behind the pawn
d = ImageDraw.Draw(bg)
gold = (217, 179, 74, 255)
def node(x, y, r=34):
    d.ellipse((x - r, y - r, x + r, y + r), fill=gold)
paths = [
    [(520, 1180), (300, 1180), (300, 760)],
    [(1528, 1180), (1748, 1180), (1748, 880), (1560, 700)],
    [(640, 1500), (360, 1500)],
]
for p in paths:
    d.line(p, fill=(217, 179, 74, 150), width=22, joint="curve")
    node(*p[-1])
node(300, 760); node(360, 1500, 28)

# pawn silhouette, white with soft shadow
pawn = Image.new("RGBA", (S, S), (0, 0, 0, 0))
pd = ImageDraw.Draw(pawn)
cx = S // 2
white = (250, 248, 244, 255)
pd.ellipse((cx - 205, 330, cx + 205, 740), fill=white)                    # head
pd.rounded_rectangle((cx - 150, 690, cx + 150, 800), radius=40, fill=white)  # collar
pd.polygon([(cx - 120, 780), (cx + 120, 780), (cx + 300, 1360), (cx - 300, 1360)], fill=white)  # body
pd.rounded_rectangle((cx - 360, 1320, cx + 360, 1470), radius=70, fill=white)  # foot
pd.rounded_rectangle((cx - 440, 1440, cx + 440, 1600), radius=80, fill=white)  # base
shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
sh = Image.new("RGBA", (S, S), (0, 0, 0, 120))
shadow.paste(sh, (0, 30), pawn.split()[3])
shadow = shadow.filter(ImageFilter.GaussianBlur(28))
bg.alpha_composite(shadow)
bg.alpha_composite(pawn)

# four-point spark, top right
def spark(cx_, cy_, r, color):
    pts = []
    for i in range(8):
        a = math.pi / 4 * i - math.pi / 2
        rr = r if i % 2 == 0 else r * 0.26
        pts.append((cx_ + rr * math.cos(a), cy_ + rr * math.sin(a)))
    ImageDraw.Draw(bg).polygon(pts, fill=color)
spark(1560, 560, 250, gold)
spark(1780, 330, 100, (255, 232, 160, 255))

# round the corners again so nothing pokes out
final = Image.new("RGBA", (S, S), (0, 0, 0, 0))
final.paste(bg, (0, 0), mask)
big = final.resize((1024, 1024), Image.LANCZOS)
sizes = {256: "icon.png", 128: "icon-128.png"}
for sz, name in sizes.items():
    big.resize((sz, sz), Image.LANCZOS).save(os.path.join(out, name))
big.save(os.path.join(out, "icon.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
big.save(os.path.join(out, "icon.icns"), sizes=[(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)])
print("wrote", os.listdir(out))
