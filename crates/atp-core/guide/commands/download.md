Saves the media of a post: images in original size, video at the best
quality offered, or just the audio. On chat platforms it saves the
attachments of a message; on a drive a file or a whole folder.

    atp bili download BV1xx411c7mD -o ~/Videos
    atp bili download '#1' --audio-only --split 25
    atp quark download /Movies -o ~/Downloads

- POST: an id, link, `url` or `#N` of the last post list; on a drive a
  path, an id or `#N` of a listing.
- `-o DIR` (default: the current directory) is created when missing. Files
  are named `<title> [<id>]`, with `-1`, `-2` ... when a post has several,
  and the extension of what actually arrived.
- `--audio-only` keeps the audio of videos (`.m4a`) and skips images.
  `--split SECS` also cuts the audio into 16 kHz mono WAV segments, in a
  folder named like the file. `ffmpeg` merges separate video and audio
  streams, extracts audio and splits.
- The result is Downloads: per file its `kind` (`image`, `video`, `audio`,
  `gif`, `file`), `path` and `bytes`.
- A post with nothing to download (for `--audio-only`: no video or audio)
  answers `not_found`.

### Cloud drives

A folder comes down with everything in it, under the drive's names (at
most 5000 files and 32 levels deep; bigger folders are refused, so take
their subfolders one by one). Bytes go to `NAME.part` first: running the
same download again continues, and a file already there with the right
size is skipped. `--audio-only` and `--split` are refused there.

More: `atp guide download`
