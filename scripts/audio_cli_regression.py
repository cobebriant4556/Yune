"""Exercise the native Yune capture CLI against a generated, independent PCM reference."""
from __future__ import annotations

import json
import math
from pathlib import Path
import struct
import subprocess
import sys
import wave

from compare_audio import compare
from yune_binary import binary_from_args


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    binary = binary_from_args(root)
    output = root / "test-results" / "audio"
    output.mkdir(parents=True, exist_ok=True)
    reference = output / "reference-tone.wav"
    captured = output / "captured-tone.wav"
    sample_rate = 48000
    samples = bytearray()
    for index in range(sample_rate):
        time = index / sample_rate
        phase = (time * 4) % 1
        envelope = math.sin(math.pi * phase) ** 2
        left = envelope * (0.3 * math.sin(2 * math.pi * 440 * time) + 0.1 * math.sin(2 * math.pi * 880 * time))
        right = envelope * 0.35 * math.sin(2 * math.pi * 660 * time)
        samples.extend(struct.pack("<hh", round(left * 32767), round(right * 32767)))
    with wave.open(str(reference), "wb") as writer:
        writer.setnchannels(2)
        writer.setsampwidth(2)
        writer.setframerate(sample_rate)
        writer.writeframes(samples)
    process = subprocess.run(
        [str(binary), "run", "examples/audio_capture.luau", str(reference), str(captured)],
        cwd=root, capture_output=True, text=True, timeout=60,
    )
    (output / "capture-cli.log").write_text(process.stdout + process.stderr, encoding="utf-8")
    if process.returncode:
        print(process.stdout + process.stderr)
        return 1
    result = compare(reference, captured, tolerance=1e-6)
    (output / "comparison.json").write_text(json.dumps(result, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, allow_nan=False))
    if result["passed"]:
        print("PASS native CLI captured 48,000 stereo frames; output matches the independent PCM reference")
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"Audio CLI regression failed: {error}", file=sys.stderr)
        raise SystemExit(1)
