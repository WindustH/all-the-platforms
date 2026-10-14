//! What ByteDance's web sites (Douyin, Toutiao, Juejin) share: the `a_bogus`
//! signer of their common web SDK ([`abogus`], with each site's ids), the
//! query encoding it signs ([`query`]), visitor identity cookies
//! ([`visitor`]) and the SSO passport's QR login ([`passport`]).

pub mod abogus;
pub mod passport;
pub mod query;
pub mod visitor;
