//! Upstream JSON → core models: users and tweets (retweets unwrapped, quotes,
//! long notes, articles, best media variants), direct messages, lists,
//! communities, notifications, trends and Spaces.

mod collections;
mod media;
mod messages;
mod notifications;
mod posts;
mod trends;
mod users;

pub use self::collections::{communities, community, list};
pub use self::messages::{chat, message, participant};
pub use self::notifications::notification;
pub use self::posts::post;
pub use self::trends::{space, trend};
pub use self::users::user;
