import math
from pathlib import Path
import struct
import tempfile
import unittest
import wave

from compare_audio import compare, read_wav


def pcm(path, values, rate=48000, channels=1):
    with wave.open(str(path), "wb") as writer:
        writer.setnchannels(channels)
        writer.setsampwidth(2)
        writer.setframerate(rate)
        writer.writeframes(struct.pack("<" + "h" * len(values), *values))


def floating(path, values):
    data = struct.pack("<" + "f" * len(values), *values)
    fmt = struct.pack("<HHIIHH", 3, 1, 48000, 192000, 4, 32)
    body = b"WAVEfmt " + struct.pack("<I", len(fmt)) + fmt + b"data" + struct.pack("<I", len(data)) + data
    path.write_bytes(b"RIFF" + struct.pack("<I", len(body)) + body)


class AudioComparisonTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.a, self.b = self.root / "a.wav", self.root / "b.wav"

    def tearDown(self):
        self.temp.cleanup()

    def test_identical_pcm(self):
        pcm(self.a, [0, 8192, -8192, 0])
        pcm(self.b, [0, 8192, -8192, 0])
        result = compare(self.a, self.b, 0)
        self.assertTrue(result["passed"])
        self.assertEqual(result["rms_error"], 0)
        self.assertEqual(result["correlation"], 1)

    def test_gain_errors_not_normalized(self):
        pcm(self.a, [0, 8192, -8192, 0])
        pcm(self.b, [0, 16384, -16384, 0])
        result = compare(self.a, self.b)
        self.assertFalse(result["passed"])
        self.assertEqual(result["first_difference_frame"], 1)
        self.assertEqual(result["max_absolute_error"], 0.25)

    def test_explicit_offset(self):
        pcm(self.a, [1000, 2000, -1000])
        pcm(self.b, [0, 0, 1000, 2000, -1000])
        self.assertFalse(compare(self.a, self.b)["passed"])
        self.assertTrue(compare(self.a, self.b, offset_frames=2)["passed"])
        self.assertTrue(compare(self.b, self.a, offset_frames=-2)["passed"])

    def test_length_difference_in_silence(self):
        pcm(self.a, [0, 0])
        pcm(self.b, [0, 0, 0])
        result = compare(self.a, self.b)
        self.assertEqual(result["rms_error"], 0)
        self.assertFalse(result["passed"])

    def test_float_and_pcm(self):
        floating(self.a, [0.25, -0.5])
        pcm(self.b, [8192, -16384])
        self.assertTrue(compare(self.a, self.b, 0)["passed"])

    def test_nonfinite_float(self):
        floating(self.a, [math.nan])
        floating(self.b, [0])
        with self.assertRaises(ValueError):
            compare(self.a, self.b)

    def test_format_mismatch(self):
        pcm(self.a, [0, 1])
        pcm(self.b, [0, 1], rate=44100)
        with self.assertRaises(ValueError):
            compare(self.a, self.b)

    def test_truncation(self):
        pcm(self.a, [0, 1])
        self.a.write_bytes(self.a.read_bytes()[:-1])
        with self.assertRaises(ValueError):
            read_wav(self.a)

    def test_empty(self):
        pcm(self.a, [])
        pcm(self.b, [])
        self.assertTrue(compare(self.a, self.b)["passed"])

    def test_bad_offset(self):
        pcm(self.a, [0])
        pcm(self.b, [0])
        with self.assertRaises(ValueError):
            compare(self.a, self.b, offset_frames=2)
        with self.assertRaises(ValueError):
            compare(self.a, self.b, offset_frames=0.5)


if __name__ == "__main__":
    unittest.main()
