## Logging in

- **QR code**: `atp quark login` shows a QR code; scan it with the Quark
  (夸克) app and confirm. ATP redeems the scan for the drive cookies. If
  Quark accepts the scan but sets no `__pus`, log in with `--browser` or
  `--cookie` instead.
- **Browser**: log in at https://pan.quark.cn, then run `atp quark login
  --browser`; it reads the cookies of quark.cn.
- **Cookie string**: copy the whole `Cookie` header of a pan.quark.cn
  request and run `atp quark login --cookie '...'`; `__pus` is required.
  `ATP_QUARK_COOKIE` supplies such a header for one run and is never
  saved.

Your drive's commands need the session. Whether `read` or `share-ls` of
someone's share answer without one is not known.

The drive renews `__puus` as you go, and ATP keeps the renewed cookie.
How long a session lasts beyond that is not known; a session the drive
no longer accepts (code 31001, HTTP 401) is `not_authenticated`, and
`atp quark login` starts a new one.

When the drive wants a risk check, ATP ends with `verification_required`
and the web drive's page: open https://pan.quark.cn, pass the check
there, then try again.

## References

- Paths of your drive: `/Movies/a.mkv`, `/Movies` (relative ones start at
  the root; `/` or `0` is the root).
- File ids (`fid`, 32 hex digits), as listings show them in `ID`.
- `#N` after any listing (`ls`, `search`, `trash`, `shares`, `share-ls`).
- Share links: `https://pan.quark.cn/s/<id>`, the code in `?pwd=`, after
  `提取码：` in pasted text, or given with `--pwd CODE`.

Paths become ids by listing folder after folder; the folders found are
cached for a day and dropped after every move, rename or delete.

## Commands

Moves, copies, deletes, shares and saves run as background tasks on the
drive; ATP waits up to three minutes for each.

### ls

`ls [PATH] [--sort name|time] [--desc]`: a folder, folders first; the
root without PATH.

### search

`search WORDS`: names across the whole drive.

### read

- `read PATH|ID`: one file or folder (size, time, md5, id).
- `read SHARE_URL`: the share's title, size, entries, views and saves,
  and when it ends.

### download

`download PATH|ID [-o DIR]`: a file, or a folder with everything in it
(up to 5000 files, 32 levels), under the drive's names. Bytes go to
`NAME.part` first; run the same download again to continue a stopped
one, and a file already there with the right size is skipped.

Download links are asked for with the Quark PC client's User-Agent, the
only one the drive hands big files to (browsers get code 23018), and
fetched with it, one connection and one 8 MB range at a time. Speed is
whatever your membership gets; ATP has no acceleration of any kind. An
expired link is renewed once in the middle of a file. Download limits
are `permission_denied`.

### upload

`upload FILE… [--to DIR]`: when the drive holds a file with the same md5
and sha1 already, the upload is done at once (`instant: true`).
Otherwise the parts go up one by one to Aliyun OSS; a stopped upload
continues at its next part when run again within 12 hours. A full drive
(32003) is `permission_denied`.

### mkdir, rename, mv, cp

`mkdir PATH [-p]`, `rename PATH NAME`, `mv SRC… DEST`, `cp SRC… DEST`.
A name that is taken (23008) is refused with a hint to pick another;
the root cannot be changed, and a share must be saved into your drive
first.

### delete

`delete PATH|ID [--yes]` moves to the recycle bin.

### trash, restore, purge

`trash` lists the recycle bin, most recently deleted first; `restore
ID…` brings entries back and `purge ID… [--yes]` deletes them for good.
Their ids are the records `trash` shows.

### quota

Storage used, free and total, the member type, and when the membership
ends.

### share

`share PATH… [--expire 0|1|7|30] [--pwd [CODE]]`: one link for the
files and folders, working for 7 days unless `--expire` says otherwise
(0: no end). `--pwd` alone makes a random code.

### shares, unshare

`shares` lists your shares; `unshare ID… [--yes]` cancels them by the
ids `shares` shows.

### share-ls

`share-ls SHARE_URL [DIR] [--pwd CODE]`: what someone's share holds;
`DIR` is a folder id (or `#N`) from an earlier `share-ls`.

### save

`save SHARE_URL [--pwd CODE] [--to DIR]` saves everything in the share
into your drive (转存). Download from your drive afterwards; ATP does
not fetch files out of shares directly. A share that wants a code
without one gives a hint to pass `--pwd`.

## Limits

- ATP waits half a second between requests; a call the drive turns away
  as too frequent is tried three times in all.
- Offline (magnet / cloud) downloads exist only in the app and are not
  supported.
