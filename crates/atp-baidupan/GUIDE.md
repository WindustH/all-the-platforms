## Two logins

Baidu Netdisk has two ways in, and one atp session holds both. Each
command takes the one that can do it and says which login it misses.

| Login | How | What it does |
| --- | --- | --- |
| Web drive (cookies `BDUSS`, `STOKEN`) | `atp baidupan login` (QR code), `--browser`, `--cookie '...'` | files and folders, search, the recycle bin, shares, saving shared files, offline downloads |
| Open platform (your own app) | `atp baidupan login --token APP_KEY:SECRET_KEY` | **downloads** and **uploads**; the file commands too when there is no web login |

- `atp baidupan login` shows a QR code; scan it with the Baidu or Baidu
  Netdisk app and confirm. `--browser` imports the cookies of a browser
  logged in at https://pan.baidu.com, `--cookie` takes them pasted
  (`BDUSS` is required, `STOKEN` is needed for shares).
- The open platform needs an app of your own: register as a developer at
  https://pan.baidu.com/union (personal developers need a real-name check),
  create an app for personal use ("个人使用"), and pass its AppKey and
  SecretKey: `login --token APP_KEY:SECRET_KEY`, or set
  `BAIDUPAN_APP_KEY` / `BAIDUPAN_SECRET_KEY` and run `login --token env`.
  atp then shows a QR code and a user code to confirm on your phone
  (polled every 5 s or slower). atp ships no app key of its own.
- The open-platform token lasts 30 days and renews itself; Baidu's
  refresh token works once, so atp writes each new one to the session at
  once. `status` shows which logins the session holds (`logins`).

## References

- Paths of your drive: `/Movies/a.mkv`, `/Movies` (relative paths start
  at the root; `/` is the root). Listings show the path as `ID`.
- File ids (`fs_id`, a bare number of 9 or more digits, in `extra`); they
  are looked up through the open platform.
- `#N` after any listing (`ls`, `search`, `trash`, `shares`, `share-ls`,
  `offline list`).
- Share links: `https://pan.baidu.com/s/1<key>` or
  `https://pan.baidu.com/share/init?surl=<key>`, the code in `?pwd=`,
  after `提取码：` in pasted text, or given with `--pwd CODE`.

## Files

- `ls [PATH] [--sort name|time|size] [--desc]`, `search WORDS`,
  `read PATH|ID`.
- `download PATH|ID [-o DIR]`: a file, or a folder with everything in it,
  under the drive's names; a stopped download continues when run again.
  **Downloads need the open-platform login.**
- `upload FILE… [--to DIR] [--overwrite]`: through the open platform. A
  file the drive holds already is done at once (`instant: true`);
  otherwise 4 MB parts go up one by one, and a stopped upload continues
  at its next part within 12 hours. Files up to 4 GB; no empty files.
- `mkdir PATH [-p]`, `rename PATH NAME`, `mv SRC… DEST`, `cp SRC… DEST`.
- `delete PATH [--yes]` moves to the recycle bin; `trash` lists it,
  `restore ID…` brings entries back, `purge ID… [--yes]` deletes them for
  good (ids are the file ids `trash` shows). Web login only.
- `quota`: storage used, free and total.

## Shares (web login)

- `share PATH… [--expire 0|1|7|30] [--pwd CODE]`: one link with an
  extraction code (a random one without `--pwd`), working 7 days unless
  `--expire` says otherwise (0: no end).
- `shares`, `unshare ID… [--yes]`.
- `read SHARE_URL`: the share's title and how many entries it holds.
- `share-ls SHARE_URL [DIR] [--pwd CODE]`: what it holds; `DIR` is a
  folder path (or `#N`) from an earlier `share-ls`.
- `save SHARE_URL [--pwd CODE] [--to DIR]` saves everything in it into
  your drive (转存). One save takes at most 500 files (VIP 3000, SVIP
  50000). Download from your drive afterwards; atp never fetches files
  out of a share.

## Offline downloads (web login)

- `offline add URL… [--to DIR]`: Baidu fetches http(s), ftp, magnet and
  ed2k links into your drive (every file of a magnet link).
- `offline list`, `offline cancel ID… [--yes]`.

## Limits

Downloads use Baidu's documented link (`dlink`) with your app's token and
`User-Agent: pan.baidu.com`, one connection and one 8 MB range at a time.
Speed is whatever your membership gets; atp has no acceleration of any
kind, signs nothing in the name of Baidu's own clients, and asks no
third-party server. A link expires after about 8 hours and is renewed in
the middle of a file. Without the open-platform login, `download` fails
with `not_authenticated`.

The open platform's own limits come back as errors with a hint:

- apps created after 2026-06-03 may download, upload, move and change
  files only under `/apps/<app name>` (`permission_denied`, codes 20020,
  31064, 20011);
- an app that was not reviewed serves its first 10 users and may get
  about 10 calls an hour (`rate_limited`, 20012); atp lists 1000 entries
  a call there;
- search: 2000 a day per user; recursive listings: 8 to 10 a minute
  (31034).

On the web route, Baidu's risk control (codes 8002, 9019) is tried again
after pauses of 3 to 20 s; when it persists the error is
`verification_required` with the drive's page to open. Moves, copies,
deletes and saves run as background tasks that atp waits for, up to five
minutes.
