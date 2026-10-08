"""Exercise the moving-source capture example without external audio assets."""
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
import wave

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "debug" / ("yune.exe" if os.name == "nt" else "yune")
OUTPUT = ROOT / "test-results" / "audio"


def capture(source: Path, name: str, effect: str, seconds: float) -> tuple[Path, dict]:
    path = OUTPUT / name
    command = [str(BINARY), "run", "examples/audio_spatial_capture.luau", str(source), str(path), str(seconds), effect]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=45)
    if result.returncode:
        raise AssertionError(f"Capture failed: {result.stdout}\n{result.stderr}")
    metadata = json.loads(Path(str(path) + ".json").read_text(encoding="utf-8"))
    assert metadata["frames"] == round(seconds * 48000), metadata
    assert metadata["sampleRate"] == 48000 and metadata["channels"] == 2
    assert metadata["effectClass"] == effect and metadata["robloxDspParity"] is False
    assert metadata["peak"] > 0 and metadata["clippedSamples"] == 0
    assert sum(step["frames"] for step in metadata["trajectory"]) == metadata["frames"]
    assert metadata["trajectory"][0]["leftGain"] > metadata["trajectory"][0]["rightGain"]
    assert all(math.isfinite(step["audibility"]) for step in metadata["trajectory"])
    assert not metadata["diagnostics"], metadata["diagnostics"]
    return path, metadata


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    source = OUTPUT / "spatial-cli-input.wav"
    samples = [round(math.sin(2 * math.pi * 523 * i / 48000) * 8000) for i in range(480)]
    with wave.open(str(source), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(48000)
        wav.writeframes(struct.pack("<" + "h" * len(samples), *samples))
    capture(source, "spatial-cli-dry.wav", "none", 0.033375)
    for effect in ["AudioEcho", "AudioReverb", "AudioPitchShifter"]:
        first, metadata = capture(source, f"spatial-cli-{effect}-1.wav", effect, 0.25)
        second, repeated = capture(source, f"spatial-cli-{effect}-2.wav", effect, 0.25)
        assert hashlib.sha256(first.read_bytes()).digest() == hashlib.sha256(second.read_bytes()).digest(), effect
        assert metadata["trajectory"] == repeated["trajectory"], effect
        assert metadata["events"] == repeated["events"], effect
        if effect == "AudioEcho":
            assert metadata["effectParameters"]["DelayTime"] == 0.12
        if effect == "AudioPitchShifter":
            assert metadata["latencySamples"] == 1024
    print("PASS spatial capture CLI: procedural WAV input, fractional frame duration, JSON trajectory, effect settings, pitch latency, and byte-identical repeat renders")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, subprocess.TimeoutExpired, ValueError) as error:
        print(f"FAIL spatial capture CLI: {error}", file=sys.stderr)
        sys.exit(1)
