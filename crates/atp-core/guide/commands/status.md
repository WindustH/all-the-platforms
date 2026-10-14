`whoami` shows the logged-in account. `status` checks whether the session
still works and says so in its exit code. `logout` forgets the saved
session.

    atp bili whoami
    atp zhihu status && atp zhihu notifications -n 10
    atp x logout

- `whoami` asks the platform and prints a User (`id`, `name`, `handle`,
  `url`, `stats` ...). Without a working session it fails with
  `not_authenticated` (exit code 3).
- `status` prints Auth: `authenticated`, `user`, `source` (`qrcode`,
  `cookie`, `browser:NAME`, `token`, or `env` for `ATP_<ID>_COOKIE`),
  `saved_at`, and a `message` on how to log in when it fails. The exit
  code is 0 when logged in and 1 when there is no session or the platform
  rejects it; other failures (network, captcha) report their own error.
  With no credentials at all it answers without asking the platform.
- Over MCP there is no exit code: read `authenticated`.
- `logout` deletes the saved `session.json` and nothing else. The platform
  still counts the login (end it in its app or website), caches stay, and
  `ATP_<ID>_COOKIE` still works. Running it twice is harmless.

More: `atp guide login`
