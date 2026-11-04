"""Makes Windows' icon (src-tauri/icons/icon.ico) fill its square, as Windows' own icons do.

The app icon (src-tauri/icons/app-icon.png, drawn by scripts/app-icon.swift) is a Mac icon: its tile sits on Apple's
icon grid, 824 px in the middle of 1024, with a margin for its shadow. `npx tauri icon` makes icon.ico from it as it
is, so on the taskbar it showed a fifth smaller than the icons beside it. This crops the artwork to the tile (dropping
the Mac's shadow) and writes icon.ico again at the sizes Windows uses. The Mac's icon.icns is left as it is.

Run from the project folder after `npx tauri icon src-tauri/icons/app-icon.png` (which writes icon.ico over):
    python3 scripts/windows-icon.py
It needs Pillow (`python3 -m pip install pillow`).
"""
from PIL import Image

SOURCE = "src-tauri/icons/app-icon.png"
TARGET = "src-tauri/icons/icon.ico"
# Apple's grid, as app-icon.swift draws the tile: 824 px from (100, 100)
TILE = (100, 100, 924, 924)
# what Windows asks an icon for: the taskbar, title bar, Start, File Explorer's views, at 100% to 200% scaling
SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (256, 256)]


def main() -> None:
    """Crop the app icon to its tile and save it as icon.ico at every size in SIZES"""
    tile = Image.open(SOURCE).convert("RGBA").crop(TILE)
    tile.resize((256, 256), Image.LANCZOS).save(TARGET, sizes=SIZES)


if __name__ == "__main__":
    main()
