"""Compare PCM or floating-point WAV captures without normalizing away errors."""
from __future__ import annotations

import argparse
from array import array
from dataclasses import dataclass
from itertools import zip_longest
import json
import math
from pathlib import Path
import struct
import sys
from typing import Iterator


@dataclass(frozen=True)
class Wav:
    path: Path
    rate: int
    channels: int
    bits: int
    encoding: int
    offset: int
    size: int

    @property
    def frames(self) -> int:
        return self.size // (self.channels * (self.bits // 8))

    def samples(self, skip_frames: int = 0) -> Iterator[float]:
        width = self.bits // 8
        skip = min(skip_frames, self.frames) * self.channels * width
        remaining = self.size - skip
        with self.path.open("rb") as stream:
            stream.seek(self.offset + skip)
            while remaining:
                count = min(remaining, 4096 * self.channels * width)
                data = stream.read(count)
                if len(data) != count:
                    raise ValueError("WAV data was truncated during comparison")
                remaining -= count
                if self.encoding == 3:
                    values = array("f" if self.bits == 32 else "d")
                    values.frombytes(data)
                    if sys.byteorder != "little":
                        values.byteswap()
                    for value in values:
                        if not math.isfinite(value):
                            raise ValueError("WAV contains non-finite floating-point samples")
                        yield float(value)
                elif self.bits == 8:
                    yield from ((value - 128) / 128 for value in data)
                elif self.bits == 24:
                    for index in range(0, len(data), 3):
                        yield int.from_bytes(data[index:index + 3], "little", signed=True) / 8388608
                else:
                    values = array("h" if self.bits == 16 else "i")
                    values.frombytes(data)
                    if values.itemsize != width:
                        raise ValueError("unsupported native integer width")
                    if sys.byteorder != "little":
                        values.byteswap()
                    scale = float(1 << (self.bits - 1))
                    yield from (value / scale for value in values)


def read_wav(path: str | Path) -> Wav:
    path = Path(path)
    length = path.stat().st_size
    with path.open("rb") as stream:
        header = stream.read(12)
        if len(header) != 12 or header[:4] != b"RIFF" or header[8:] != b"WAVE":
            raise ValueError(f"{path}: expected a little-endian RIFF/WAVE file")
        end = struct.unpack_from("<I", header, 4)[0] + 8
        if end > length or end < 12:
            raise ValueError(f"{path}: invalid RIFF size")
        fmt = None
        payload = None
        while stream.tell() + 8 <= end:
            kind, size = struct.unpack("<4sI", stream.read(8))
            position = stream.tell()
            if position + size > end:
                raise ValueError(f"{path}: truncated WAV chunk")
            if kind == b"fmt ":
                if size < 16 or size > 65536:
                    raise ValueError("invalid WAV format chunk")
                fmt = stream.read(size)
            elif kind == b"data":
                if payload is not None:
                    raise ValueError("multiple WAV data chunks are not supported")
                payload = (position, size)
            stream.seek(position + size + (size & 1))
    if fmt is None or payload is None:
        raise ValueError("WAV requires format and data chunks")
    encoding, channels, rate, _, align, bits = struct.unpack_from("<HHIIHH", fmt)
    if encoding == 0xFFFE:
        if len(fmt) < 40 or struct.unpack_from("<H", fmt, 16)[0] < 22:
            raise ValueError("invalid extensible WAV format")
        encoding = struct.unpack_from("<I", fmt, 24)[0]
        if fmt[28:40] != bytes.fromhex("00001000800000aa00389b71"):
            raise ValueError("unsupported extensible WAV subformat")
        valid_bits = struct.unpack_from("<H", fmt, 18)[0]
        if valid_bits not in (0, bits):
            raise ValueError("WAV valid-bit packing differs from container width")
    if encoding not in (1, 3) or (encoding == 1 and bits not in (8, 16, 24, 32)) or (encoding == 3 and bits not in (32, 64)):
        raise ValueError("only integer PCM and IEEE float WAV are supported")
    if not 1 <= channels <= 32 or rate <= 0 or align != channels * (bits // 8):
        raise ValueError("invalid WAV channel/rate/block alignment")
    if payload[1] % align:
        raise ValueError("WAV data ends in a partial frame")
    return Wav(path, rate, channels, bits, encoding, *payload)


def compare(reference: str | Path, actual: str | Path, tolerance: float = 1e-6, offset_frames: int = 0) -> dict:
    if not math.isfinite(tolerance) or tolerance < 0:
        raise ValueError("tolerance must be finite and nonnegative")
    if not isinstance(offset_frames, int):
        raise ValueError("offset_frames must be an integer")
    ref, out = read_wav(reference), read_wav(actual)
    if ref.rate != out.rate or ref.channels != out.channels:
        raise ValueError("sample rates and channel counts must match; comparison never resamples implicitly")
    skip_ref, skip_out = max(0, -offset_frames), max(0, offset_frames)
    if skip_ref > ref.frames or skip_out > out.frames:
        raise ValueError("offset exceeds an input's duration")
    count = mismatches = 0
    squared_error = squared_ref = squared_out = dot = absolute_error = signed_error = 0.0
    max_error = ref_peak = out_peak = 0.0
    first = last = None
    for index, (a, b) in enumerate(zip_longest(ref.samples(skip_ref), out.samples(skip_out), fillvalue=0.0)):
        difference = b - a
        error = abs(difference)
        squared_error += difference * difference
        absolute_error += error
        signed_error += difference
        squared_ref += a * a
        squared_out += b * b
        dot += a * b
        max_error = max(max_error, error)
        ref_peak = max(ref_peak, abs(a))
        out_peak = max(out_peak, abs(b))
        count += 1
        if error > tolerance:
            mismatches += 1
            frame = index // ref.channels
            if first is None:
                first = frame
            last = frame
    length_matches = ref.frames - skip_ref == out.frames - skip_out
    denominator = max(count, 1)
    snr = 10 * math.log10(squared_ref / squared_error) if squared_error > 0 and squared_ref > 0 else None
    correlation = dot / math.sqrt(squared_ref * squared_out) if squared_ref > 0 and squared_out > 0 else None
    return {
        "passed": mismatches == 0 and length_matches,
        "reference": str(ref.path), "actual": str(out.path),
        "sample_rate": ref.rate, "channels": ref.channels,
        "reference_frames": ref.frames, "actual_frames": out.frames,
        "reference_duration": ref.frames / ref.rate, "actual_duration": out.frames / ref.rate,
        "offset_frames": offset_frames, "tolerance": tolerance,
        "aligned_length_matches": length_matches, "compared_samples": count,
        "mismatching_samples": mismatches, "first_difference_frame": first,
        "last_difference_frame": last, "max_absolute_error": max_error,
        "rms_error": math.sqrt(squared_error / denominator),
        "mean_absolute_error": absolute_error / denominator,
        "dc_error": signed_error / denominator,
        "reference_peak": ref_peak, "actual_peak": out_peak,
        "reference_rms": math.sqrt(squared_ref / denominator),
        "actual_rms": math.sqrt(squared_out / denominator),
        "snr_db": snr, "correlation": correlation,
        "normalization_applied": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--tolerance", type=float, default=1e-6)
    parser.add_argument("--offset-frames", type=int, default=0, help="explicit alignment only: positive skips frames in actual; negative skips reference")
    parser.add_argument("--json", type=Path, dest="json_path")
    args = parser.parse_args()
    try:
        report = compare(args.reference, args.actual, args.tolerance, args.offset_frames)
        encoded = json.dumps(report, indent=2, allow_nan=False) + "\n"
        print(encoded, end="")
        if args.json_path:
            args.json_path.parent.mkdir(parents=True, exist_ok=True)
            args.json_path.write_text(encoded, encoding="utf-8")
        return 0 if report["passed"] else 1
    except (OSError, ValueError, struct.error) as error:
        print(f"Audio comparison failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
