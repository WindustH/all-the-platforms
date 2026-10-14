## Logging in

Baidu Netdisk has two ways in, and one ATP session holds both. Each
command takes the one that can do it and says which login it misses;
commands both can do use the web drive when its login is there (it
spares the app's call quota).

| Login | How | What it does |
| --- | --- | --- |
| Web drive (cookies `BDUSS`, `STOKEN`) | `atp baidupan login` (QR code), `--browser`, `--cookie '...'` | files and folders, search, the recycle bin, shares, saving shared files, offline downloads |
| Open platform (your own app) | `atp baidupan login --token APP_KEY:SECRET_KEY` | **downloads** and **uploads**; the file commands too when there is no web login |

Nothing works without a login.

### Web drive

- **QR code**: `atp baidupan login` shows a QR code; scan it with the
  Baidu or Baidu Netdisk app and confirm. Baidu's passport then hands the
  drive its own `STOKEN`, which shares need. If Baidu accepts the scan
  but sets no `BDUSS`, use `--browser` or `--cookie` instead.
- **Browser**: log in at https://pan.baidu.com, then run `atp baidupan
  login --browser`. It reads the cookies of pan.baidu.com and baidu.com;
  the drive's own `STOKEN` wins over passport's.
- **Cookie string**: copy the `Cookie` header of a pan.baidu.com request
  and run `atp baidupan login --cookie 'BDUSS=...; STOKEN=...'`. `BDUSS`
  is required, `STOKEN` is needed for shares. `ATP_BAIDUPAN_COOKIE`
  supplies such a header for one run and is never saved.

`login` checks the cookies with the web drive before saving them. How
long a web login lasts is not known; when Baidu stops accepting it,
commands fail with `not_authenticated` ("Baidu does not accept this web
login") and the hint to log in again. `whoami` then falls back to the
open platform when the session holds it.

### Open platform

The open platform needs an app of your own; ATP ships no app key.

1. Register as a developer at https://pan.baidu.com/union (personal
   developers need a real-name check) and create an app for personal use
   ("个人使用").
2. Run `atp baidupan login --token APP_KEY:SECRET_KEY` with its AppKey
   and SecretKey. Or set `BAIDUPAN_APP_KEY` and `BAIDUPAN_SECRET_KEY` and
   run `atp baidupan login --token env`.
3. ATP shows a QR code and a user code: scan the code with your phone, or
   open the page it names and enter the code, and allow the app
   (`basic,netdisk`). ATP polls every 5 s or slower until you confirm.

The access token lasts 30 days and renews itself with the app's key and
secret. Baidu's refresh token works once, so ATP writes each new one to
the session at once. A key and secret passed to `--token` are kept with
the session for that; with `--token env` they are not written, so the
two variables must stay set for renewals. A token that cannot be renewed
fails with `not_authenticated`; run `login --token` again.

`whoami` and `status` show which logins the session holds (`logins`).

### Risk control

On the web route, Baidu's risk control (codes 8002, 9019) and the
per-minute limit (31034) are tried again after pauses of 3 to 20 s; when
a check persists the error is `verification_required` with the drive's
page: open https://pan.baidu.com in a browser logged in to this account,
pass the check, then try again in a few minutes. An account Baidu has
flagged (9013) gives `permission_denied`; the customer service in Baidu
Netdisk's app can lift it. ATP waits half a second between requests.

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

## Commands

Moves, copies, deletes and saves run on the web route as background
tasks that ATP waits for, up to five minutes.

### ls

`ls [PATH] [--sort name|time|size] [--desc]`: a folder, folders first;
either login.

### search

Names across the drive; either login. The open platform allows 2000
searches a day per user.

### read

- `read PATH|ID`: one file or folder; either login.
- `read SHARE_URL`: the share's title and how many entries it holds
  (web login).

### download

`download PATH|ID [-o DIR]`: a file, or a folder with everything in it,
under the drive's names; a stopped download continues when run again.
**Downloads need the open-platform login**; without it `download` fails
with `not_authenticated`.

Downloads use Baidu's documented link (`dlink`) with your app's token
and `User-Agent: pan.baidu.com`, one connection and one 8 MB range at a
time. Speed is whatever your membership gets; ATP has no acceleration of
any kind, signs nothing in the name of Baidu's own clients, and asks no
third-party server. A link expires after about 8 hours and is renewed in
the middle of a file. Files are never fetched out of a share: `save` it
first.

### upload

`upload FILE… [--to DIR] [--overwrite]` goes through the open platform
only. A file the drive holds already is done at once (`instant: true`);
otherwise 4 MB parts go up one by one, and a stopped upload continues at
its next part within 12 hours. Files up to 4 GB; no empty files.

### mkdir, rename, mv, cp

`mkdir PATH [-p]`, `rename PATH NAME`, `mv SRC… DEST`, `cp SRC… DEST`;
either login. A name that is taken (-8, 31061) is refused.

### delete

`delete PATH [--yes]` moves to the recycle bin; either login.

### trash, restore, purge

`trash` lists the recycle bin, `restore ID…` brings entries back and
`purge ID… [--yes]` deletes them for good; the ids are the file ids
`trash` shows. Web login only.

### quota

Storage used, free and total; either login.

### share

`share PATH… [--expire 0|1|7|30] [--pwd CODE]` (web login): one link
with an extraction code (a random one without `--pwd`), working 7 days
unless `--expire` says otherwise (0: no end).

### shares, unshare

`shares` lists your shares and `unshare ID… [--yes]` cancels them (web
login).

### share-ls

`share-ls SHARE_URL [DIR] [--pwd CODE]` (web login): what someone's
share holds; `DIR` is a folder path (or `#N`) from an earlier
`share-ls`. A code opens the share for this run only.

### save

`save SHARE_URL [--pwd CODE] [--to DIR]` (web login) saves everything in
the share into your drive (转存). One save takes at most 500 files (VIP
3000, SVIP 50000); save the share's folders one by one beyond that. A
target folder that holds entries of the same names is refused; save into
another with `--to`. Download from your drive afterwards.

### offline

Offline downloads (离线下载) need the web login. How many run at once
depends on the membership; Baidu refuses the ones over it.

### offline add

`offline add URL… [--to DIR]`: Baidu fetches http(s), ftp, magnet and
ed2k links into your drive (every file of a magnet link).

### offline list, offline cancel

`offline list` shows the tasks and how far they got; `offline cancel
ID… [--yes]` cancels them by the ids `offline list` shows.

## Limits

The open platform's own limits come back as errors with a hint:

- apps created after 2026-06-03 may download, upload, move and change
  files only under `/apps/<app name>` (`permission_denied`, codes 20020,
  31064, 20011);
- an app that was not reviewed serves its first 10 users and may get
  about 10 calls an hour (`rate_limited`, 20012); ATP lists 1000 entries
  a call there;
- search: 2000 a day per user; recursive listings: 8 to 10 a minute
  (31034).
