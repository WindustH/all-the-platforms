Creator analytics of the logged-in account, or with POST of one of your
posts, as the platform's creator center reports them.

    atp bili insights                   # your account, last 30 days
    atp bili insights --days 7 -f csv > week.csv
    atp x insights 2104695667180380260  # one of your tweets

- `--days N` (default 30) sets the window of the trends; platforms round
  it to the windows they offer.
- The result is Insights: `kind` (`account` or `post`), `subject`,
  `from` / `to`, `totals` (headline numbers, rates as fractions 0..1;
  `totals_period` when they cover another span, for a post usually
  `lifetime`), `series` (one daily trend per metric), `breakdowns`
  (traffic source, device, age, region ..., with `period`), and
  `warnings` saying what the platform withheld.
- The table shows totals, trends and breakdowns. CSV and JSON Lines give
  one row per number, told apart by `section` (`total`, `series`,
  `breakdown`, `warning`).
- Someone else's post gets its public counters with a warning, where the
  platform has that.
- `atp platforms` lists account analytics (`insights`) and post analytics
  (`post-insights`) separately; a platform may have only one of them.
- It needs a login; some platforms need more (a paid tier, an OAuth app, a
  follower threshold), named in `warnings` or in the platform's notes.

More: `atp guide analysis`
