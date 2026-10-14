//! What ByteDance's web sites (Douyin, Toutiao, Juejin, TikTok) share: the
//! `a_bogus` signer of their common web SDK ([`abogus`], with each site's
//! ids), the query encoding it signs ([`query`]), visitor identity cookies
//! ([`visitor`]), the passports' QR and phone logins ([`passport`]), the
//! key pair logins register for signed writes ([`ticket_guard`]) and the
//! live rooms' push protocol with its signature ([`webcast`]).

pub mod abogus;
pub mod passport;
pub mod query;
pub mod ticket_guard;
pub mod visitor;
pub mod webcast;
