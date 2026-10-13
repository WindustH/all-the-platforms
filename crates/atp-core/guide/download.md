# Downloading

    atp <platform> download POST [-o DIR] [--audio-only] [--split SECS]

- Images come in their original size, videos at the best quality offered.
  Videos served as separate video and audio streams are merged with
  `ffmpeg` (kept as two files when ffmpeg is missing).
- `--audio-only` keeps the audio track (`.m4a`).
- `--split SECS` also cuts the audio into 16 kHz mono WAV segments of that
  length in a folder next to the file, ready for speech recognition.
- Files are named `<title> [<id>]` with the extension of what was actually
  received; posts with several files get `-1`, `-2` ...
- Large files come in 8 MB ranges, each with its own timeout and retries.

Examples:

    atp bili download BV1xx411c7mD
    atp bili download BV1xx411c7mD --audio-only --split 25 -o ~/asr
    atp xhs download '#1'                      # every image of a note
    atp x download https://x.com/NASA/status/2104695667180380260
    atp yt download dQw4w9WgXcQ --audio-only

## Requirements

- `ffmpeg` on PATH for merging, audio extraction from videos, and `--split`.
- YouTube: some videos only serve full streams to clients with a
  proof-of-origin token; all-the-platforms then asks `yt-dlp` (when installed) for
  the stream URLs. Install yt-dlp with a JavaScript runtime for those.

The result lists every file with its kind, path and size.
