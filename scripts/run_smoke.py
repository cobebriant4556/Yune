import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
binary = root / "target" / "debug" / ("yune.exe" if os.name == "nt" else "yune")
output = root / "test-results"
output.mkdir(exist_ok=True)
scenes = ["require_regression", "runtime_smoke", "render_smoke", "visual_smoke"]
results = []
for scene in scenes:
    log = output / f"{scene}.log"
    try:
        process = subprocess.run([str(binary), "run", f"examples/{scene}.luau"], cwd=root, capture_output=True, text=True, timeout=60)
        text = process.stdout + process.stderr
        passed = process.returncode == 0
    except subprocess.TimeoutExpired as error:
        text = f"TIMEOUT after 60 seconds: {error}\n"
        passed = False
    log.write_text(text, encoding="utf-8")
    results.append({"test": scene, "passed": passed, "log": str(log.relative_to(root))})
    print(f"{'PASS' if passed else 'FAIL'} {scene}", flush=True)
    if not passed:
        print(text[-12000:], flush=True)
(output / "summary.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
sys.exit(0 if all(item["passed"] for item in results) else 1)
