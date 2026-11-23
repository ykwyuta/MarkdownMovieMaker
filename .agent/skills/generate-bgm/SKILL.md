---
name: generate-bgm
description: Synthesizes original, royalty-free, loopable BGM tracks (MP3) using Python standard library and ffmpeg for MarkdownMovieMaker.
---

# Generate BGM Skill

Generates original, copyright-free, loopable background music tracks (`001.mp3`, `002.mp3`, `003.mp3`, `004.mp3`, `default_bgm.mp3`) suitable for explanation and talk videos.

## Usage

Run the bundled Python script from the repository root:

```powershell
python .agent/skills/generate-bgm/scripts/generate_bgm.py --out-dir data/bgm
```

## Requirements

- Python 3.8+ (standard library only)
- `ffmpeg` on `PATH`
