//! What all-the-platforms's Meituan and Dianping share. Both sites sit
//! behind one protection stack: the MTSI gateway, the Yoda human check on
//! `verify.meituan.com` and the `mtgsig` header of H5guard.
//!
//! - [`guard`]: the one path every page and XHR of either site takes:
//!   cookies, `Referer`, the H5 `csec` flags and the rohr `_token`.
//! - [`refusal`]: how the gateway says no, as atp errors.
//! - [`rohr`]: the `_token`, zlib and base64 over a small JSON.
//! - [`shops`]: Dianping shop references and the mobile shop page.
//! - [`reviews`]: Dianping's review list; Meituan's 到店 shops show the
//!   same reviews, read from Dianping's backend.
//! - [`account`]: who a Dianping session belongs to.
//! - [`geo`]: Meituan's city and area lookups, open to visitors.
//!
//! atp does not make `mtgsig` and does not run H5guard: forging it to get
//! past risk control is the conduct Chinese courts have punished. A call
//! the gateway refuses ends in `verification_required` with the page to
//! open; the person passes the check in their own browser and atp retries
//! with that browser's cookies.

pub mod account;
pub mod geo;
pub mod guard;
pub mod refusal;
pub mod reviews;
pub mod rohr;
pub mod shops;
