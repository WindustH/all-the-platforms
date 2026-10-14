## Logging in

- `atp quark login` shows a QR code; scan it with the Quark (夸克) app and
  confirm. atp redeems the scan for the drive cookies.
- `atp quark login --browser` imports the cookies of a browser logged in
  at https://pan.quark.cn, and `--cookie '...'` takes them pasted (the
  whole `Cookie` header of a pan.quark.cn request; `__pus` is required).
- The drive renews `__puus` as you go; atp keeps the renewed cookie. A
  session the drive no longer accepts (code 31001) is `not_authenticated`.

## References

- Paths of your drive: `/Movies/a.mkv`, `/Movies` (relative ones start at
  the root; `/` or `0` is the root).
- File ids (`fid`, 32 hex digits), as listings show them in `ID`.
- `#N` after any listing (`ls`, `search`, `trash`, `shares`, `share-ls`).
- Share links: `https://pan.quark.cn/s/<id>`, the code in `?pwd=`, after
  `提取码：` in pasted text, or given with `--pwd CODE`.

Paths become ids by listing folder after folder; the folders found are
cached for a day and dropped after every move, rename or delete.

## Files

- `ls [PATH] [--sort name|time] [--desc]`: a folder, folders first.
- `search WORDS`: names across the whole drive.
- `read PATH|ID`: one file or folder (size, time, md5, id).
- `download PATH|ID [-o DIR]`: a file, or a folder with everything in it
  (up to 5000 files, 32 levels), under the drive's names. Bytes go to
  `NAME.part` first; run the same download again to continue a stopped
  one, and a file already there with the right size is skipped.
- `upload FILE… [--to DIR]`: when the drive holds a file with the same md5
  and sha1 already, the upload is done at once (`instant: true`).
  Otherwise the parts go up one by one; a stopped upload continues at its
  next part when run again within 12 hours.
- `mkdir PATH [-p]`, `rename PATH NAME`, `mv SRC… DEST`, `cp SRC… DEST`.
- `delete PATH|ID [--yes]` moves to the recycle bin; `trash` lists it,
  `restore ID…` brings entries back and `purge ID… [--yes]` deletes them
  for good. Their ids are the records `trash` shows.
- `quota`: storage used, free and total, and the member type.

## Shares

- `share PATH… [--expire 0|1|7|30] [--pwd [CODE]]`: one link for the
  files and folders, working for 7 days unless `--expire` says otherwise
  (0: no end). `--pwd` alone makes a random code.
- `shares`, `unshare ID… [--yes]`.
- `read SHARE_URL`: the share's title, size, entries, views and saves,
  and when it ends.
- `share-ls SHARE_URL [DIR] [--pwd CODE]`: what it holds; `DIR` is a
  folder id (or `#N`) from an earlier `share-ls`.
- `save SHARE_URL [--pwd CODE] [--to DIR]` saves everything in it into
  your drive (转存). Download from your drive afterwards; atp does not
  fetch files out of shares directly.

## Downloads and limits

Download links are asked for with the Quark PC client's User-Agent, the
only one the drive hands big files to (browsers get code 23018), and
fetched with it, one connection and one 8 MB range at a time. Speed is
whatever your membership gets; atp has no acceleration of any kind. An
expired link is renewed once in the middle of a file.

Moves, copies, deletes, shares and saves run as background tasks on the
drive; atp waits up to three minutes for each. A full drive (32003) and
download limits are `permission_denied`; a risk check is
`verification_required` with the drive's page to open.

Offline (magnet / cloud) downloads exist only in the app and are not
supported.
