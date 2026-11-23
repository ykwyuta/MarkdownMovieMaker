#!/usr/bin/env python3
"""Generate original, royalty-free, loopable BGM tracks (MP3) using Python + ffmpeg."""

import argparse
import math
import struct
import subprocess
import tempfile
import wave
from pathlib import Path

SAMPLE_RATE = 44100


def midi_to_freq(midi: float) -> float:
    return 440.0 * (2.0 ** ((midi - 69.0) / 12.0))


def render_track(
    bpm: float,
    bars: int,
    chords: list[list[int]],
    melody: list[tuple[float, float, int]],
    bass_pattern: list[tuple[float, float, int]],
    tone: str = "epiano",
) -> list[float]:
    beats_per_bar = 4
    total_beats = bars * beats_per_bar
    sec_per_beat = 60.0 / bpm
    total_samples = int(total_beats * sec_per_beat * SAMPLE_RATE)
    buf = [0.0] * total_samples

    def add_note(start_beat: float, dur_beat: float, midi: int, amp: float, timbre: str):
        freq = midi_to_freq(midi)
        start_idx = int(start_beat * sec_per_beat * SAMPLE_RATE)
        dur_samples = int(dur_beat * sec_per_beat * SAMPLE_RATE)
        attack = max(1, int(0.015 * SAMPLE_RATE))
        release = max(1, int(0.06 * SAMPLE_RATE))
        for i in range(dur_samples):
            idx = start_idx + i
            if idx >= total_samples:
                break
            t = i / SAMPLE_RATE
            if i < attack:
                env = i / attack
            elif i > dur_samples - release:
                env = (dur_samples - i) / release
            else:
                env = 1.0
            decay = math.exp(-t * (2.2 if timbre == "pluck" else 0.9))
            phase = 2.0 * math.pi * freq * t
            if timbre == "epiano":
                val = (
                    0.65 * math.sin(phase)
                    + 0.25 * math.sin(2.0 * phase) * math.exp(-t * 3.0)
                    + 0.10 * math.sin(3.0 * phase) * math.exp(-t * 5.0)
                )
            elif timbre == "pluck":
                val = (
                    0.6 * math.sin(phase)
                    + 0.3 * math.sin(2.0 * phase)
                    + 0.1 * math.sin(4.0 * phase)
                )
            elif timbre == "pad":
                val = (
                    0.7 * math.sin(phase)
                    + 0.2 * math.sin(2.0 * phase)
                    + 0.1 * math.sin(0.5 * phase)
                )
            else:  # bass
                val = 0.8 * math.sin(phase) + 0.2 * math.sin(2.0 * phase)
            buf[idx] += amp * env * decay * val

    # Chords (1 chord per bar, cycling)
    for bar in range(bars):
        chord = chords[bar % len(chords)]
        for note in chord:
            add_note(bar * 4.0, 3.85, note, 0.08, "pad" if tone == "pad" else "epiano")

    # Bass
    for bar in range(bars):
        root = chords[bar % len(chords)][0] - 12
        for offset, dur, interval in bass_pattern:
            add_note(bar * 4.0 + offset, dur, root + interval, 0.14, "bass")

    # Melody
    mel_len = len(melody)
    for idx_m, (start_b, dur_b, midi) in enumerate(melody):
        if start_b < total_beats:
            add_note(start_b, dur_b, midi, 0.18, tone)

    # Soft normalize to -3 dB
    peak = max((abs(x) for x in buf), default=1.0)
    if peak > 0:
        scale = 0.7 / peak
        buf = [x * scale for x in buf]
    return buf


def save_mp3(samples: list[float], out_path: Path) -> None:
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
        wav_path = Path(tmp.name)
    try:
        with wave.open(str(wav_path), "wb") as wf:
            wf.setnchannels(1)
            wf.setsampwidth(2)
            wf.setframerate(SAMPLE_RATE)
            pcm = bytearray(len(samples) * 2)
            for i, s in enumerate(samples):
                v = max(-32767, min(32767, int(s * 32767)))
                struct.pack_into("<h", pcm, i * 2, v)
            wf.writeframes(pcm)
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-i",
                str(wav_path),
                "-codec:a",
                "libmp3lame",
                "-b:a",
                "192k",
                str(out_path),
            ],
            check=True,
        )
    finally:
        wav_path.unlink(missing_ok=True)


def build_tracks() -> dict[str, list[float]]:
    tracks: dict[str, list[float]] = {}

    # default_bgm.mp3: Gentle cheerful C major loop (110 BPM, 16 bars ~ 35s)
    chords_def = [[60, 64, 67], [65, 69, 72], [67, 71, 74], [60, 64, 67]]
    mel_def = []
    motif_def = [
        (0.0, 0.5, 72), (0.5, 0.5, 74), (1.0, 1.0, 76), (2.0, 0.5, 79), (2.5, 0.5, 76), (3.0, 1.0, 74),
        (4.0, 0.5, 72), (4.5, 0.5, 69), (5.0, 1.0, 72), (6.0, 1.0, 74), (7.0, 1.0, 72),
        (8.0, 0.5, 74), (8.5, 0.5, 76), (9.0, 1.0, 79), (10.0, 0.5, 76), (10.5, 0.5, 74), (11.0, 1.0, 71),
        (12.0, 1.5, 72), (13.5, 0.5, 74), (14.0, 2.0, 72),
    ]
    for rep in range(4):
        for s, d, m in motif_def:
            mel_def.append((rep * 16.0 + s, d, m))
    tracks["default_bgm.mp3"] = render_track(
        bpm=110, bars=16, chords=chords_def, melody=mel_def,
        bass_pattern=[(0.0, 1.5, 0), (2.0, 1.5, 7)], tone="epiano",
    )

    # 001.mp3: Upbeat bright explanation in G major (120 BPM, 16 bars ~ 32s)
    chords_001 = [[55, 59, 62], [60, 64, 67], [62, 66, 69], [55, 59, 62]]
    mel_001 = []
    motif_001 = [
        (0.0, 0.5, 71), (0.5, 0.5, 74), (1.0, 0.5, 79), (1.5, 0.5, 76), (2.0, 1.0, 74), (3.0, 1.0, 71),
        (4.0, 0.5, 72), (4.5, 0.5, 76), (5.0, 1.0, 79), (6.0, 1.0, 76), (7.0, 1.0, 72),
        (8.0, 0.5, 74), (8.5, 0.5, 78), (9.0, 1.0, 81), (10.0, 1.0, 78), (11.0, 1.0, 74),
        (12.0, 1.0, 79), (13.0, 1.0, 76), (14.0, 2.0, 74),
    ]
    for rep in range(4):
        for s, d, m in motif_001:
            mel_001.append((rep * 16.0 + s, d, m))
    tracks["001.mp3"] = render_track(
        bpm=120, bars=16, chords=chords_001, melody=mel_001,
        bass_pattern=[(0.0, 0.9, 0), (1.0, 0.9, 7), (2.0, 0.9, 0), (3.0, 0.9, 7)], tone="pluck",
    )

    # 002.mp3: Calm lo-fi electric piano in F major (95 BPM, 12 bars ~ 30s)
    chords_002 = [[53, 57, 60, 64], [55, 58, 62, 65], [57, 60, 64, 67], [58, 62, 65, 69]]
    mel_002 = []
    motif_002 = [
        (0.0, 1.5, 69), (1.5, 0.5, 72), (2.0, 2.0, 74),
        (4.0, 1.5, 72), (5.5, 0.5, 69), (6.0, 2.0, 67),
        (8.0, 1.0, 69), (9.0, 1.0, 72), (10.0, 2.0, 76),
        (12.0, 1.5, 74), (13.5, 0.5, 72), (14.0, 2.0, 69),
    ]
    for rep in range(3):
        for s, d, m in motif_002:
            mel_002.append((rep * 16.0 + s, d, m))
    tracks["002.mp3"] = render_track(
        bpm=95, bars=12, chords=chords_002, melody=mel_002,
        bass_pattern=[(0.0, 3.5, 0)], tone="epiano",
    )

    # 003.mp3: Playful quirky staccato in D major (116 BPM, 16 bars ~ 33s)
    chords_003 = [[62, 66, 69], [59, 62, 66], [55, 59, 62], [57, 61, 64]]
    mel_003 = []
    motif_003 = [
        (0.0, 0.4, 74), (0.5, 0.4, 78), (1.0, 0.4, 81), (1.5, 0.4, 78), (2.0, 0.8, 74), (3.0, 0.8, 73),
        (4.0, 0.4, 71), (4.5, 0.4, 74), (5.0, 0.8, 78), (6.0, 0.8, 74), (7.0, 0.8, 71),
        (8.0, 0.4, 71), (8.5, 0.4, 74), (9.0, 0.4, 79), (9.5, 0.4, 78), (10.0, 0.8, 74), (11.0, 0.8, 71),
        (12.0, 0.8, 73), (13.0, 0.8, 76), (14.0, 1.6, 74),
    ]
    for rep in range(4):
        for s, d, m in motif_003:
            mel_003.append((rep * 16.0 + s, d, m))
    tracks["003.mp3"] = render_track(
        bpm=116, bars=16, chords=chords_003, melody=mel_003,
        bass_pattern=[(0.0, 0.45, 0), (1.0, 0.45, 7), (2.0, 0.45, 0), (3.0, 0.45, 7)], tone="pluck",
    )

    # 004.mp3: Warm thoughtful ambient in A minor / C major (90 BPM, 12 bars ~ 32s)
    chords_004 = [[57, 60, 64], [53, 57, 60], [60, 64, 67], [55, 59, 62]]
    mel_004 = []
    motif_004 = [
        (0.0, 2.0, 72), (2.0, 1.0, 71), (3.0, 1.0, 69),
        (4.0, 2.0, 69), (6.0, 2.0, 67),
        (8.0, 1.5, 67), (9.5, 0.5, 69), (10.0, 2.0, 72),
        (12.0, 2.0, 71), (14.0, 2.0, 69),
    ]
    for rep in range(3):
        for s, d, m in motif_004:
            mel_004.append((rep * 16.0 + s, d, m))
    tracks["004.mp3"] = render_track(
        bpm=90, bars=12, chords=chords_004, melody=mel_004,
        bass_pattern=[(0.0, 3.8, 0)], tone="pad",
    )

    return tracks


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate royalty-free loopable BGM tracks.")
    parser.add_argument("--out-dir", type=Path, default=Path("data/bgm"))
    args = parser.parse_args()

    tracks = build_tracks()
    for filename, samples in tracks.items():
        out_path = args.out_dir / filename
        save_mp3(samples, out_path)
        print(f"Generated {out_path} ({len(samples) / SAMPLE_RATE:.1f}s)")


if __name__ == "__main__":
    main()
