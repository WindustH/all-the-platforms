//! Changing published posts (`edit`): videos through the creator
//! center's edit page ([`video`]), dynamics through t.bilibili.com's
//! editor ([`dynamic`]). Both load the post first, print the change and
//! ask before anything goes out. Articles are not edited.

mod dynamic;
mod video;

pub use self::dynamic::edit as dynamic;
pub use self::video::edit as video;
