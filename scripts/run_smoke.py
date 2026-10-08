import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import zlib

root = Path(__file__).resolve().parents[1]
binary = root / "target" / "debug" / ("yune.exe" if os.name == "nt" else "yune")
output = root / "test-results"
assets = output / "assets"
assets.mkdir(parents=True, exist_ok=True)
shutil.copyfile(root / "examples/assets/triangle.mesh", assets / "123.mesh")

def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

scanline = b"\x00" + bytes([255, 0, 0, 255]) * 2
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(scanline * 2)) + chunk(b"IEND", b"")
(assets / "456").write_bytes(png)
scenes = ["require_regression", "runtime_smoke", "cframe_regression", "render_smoke", "visual_smoke", "asset_regression", "rig_regression", "gui_regression", "defaults_regression", "audio_regression", "audio_edges_regression", "audio_spatial_regression", "audio_effects_regression"]
results = []

def run_test(name, command, timeout=60):
    log = output / f"{name}.log"
    try:
        process = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=timeout)
        text = process.stdout + process.stderr
        passed = process.returncode == 0
    except (subprocess.TimeoutExpired, OSError) as error:
        text = f"Execution failed: {error}\n"
        passed = False
    log.write_text(text, encoding="utf-8")
    results.append({"test": name, "passed": passed, "log": str(log.relative_to(root))})
    print(f"{'PASS' if passed else 'FAIL'} {name}", flush=True)
    if not passed:
        print(text[-12000:], flush=True)

for scene in scenes:
    run_test(scene, [str(binary), "run", f"examples/{scene}.luau"])
run_test("audio_cli_regression", [sys.executable, "scripts/audio_cli_regression.py"], timeout=90)
(output / "summary.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
sys.exit(0 if all(item["passed"] for item in results) else 1)
