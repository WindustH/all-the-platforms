//! ATP (all-the-platforms) as a Rust library: the platforms of the `atp`
//! command for your own programs, with the same sessions, signing and
//! normalized data.
//!
//! Each platform is a feature and a module (`zhihu`, `bilibili`, `douyin`
//! ...); pick the ones you need:
//!
//! ```toml
//! [dependencies]
//! atp = { git = "https://github.com/WindustH/atp", default-features = false, features = ["zhihu"] }
//! tokio = { version = "1", features = ["rt", "macros"] }
//! ```
//!
//! [`open`] gives a platform with the session `atp <platform> login` saved
//! (or a guest without one); [`Open`] takes cookies, a proxy or a timeout in
//! code. Every command of the CLI is a [`Platform`] method returning typed
//! data ([`Post`], [`User`], [`Page`] ...), and a platform's own commands
//! run through [`Platform::run_extra`] with its `Command` enum.
//!
//! ```no_run
//! use atp::zhihu::Zhihu;
//! use atp::{PageReq, Platform};
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> atp::Result<()> {
//!   let zhihu: Zhihu = atp::open()?;
//!   let hot = zhihu.hot(None, &PageReq::first(10)).await?;
//!   for post in &hot.items {
//!     println!("{} {}", post.id, post.title.as_deref().unwrap_or_default());
//!   }
//!   // Up to 100 posts of a user, page after page.
//!   let all = atp::paging::collect(100, None, async |page| zhihu.user_posts("zhihuadmin", &page).await).await?;
//!   println!("{} posts", all.items.len());
//!   Ok(())
//! }
//! ```
//!
//! The futures borrow the platform, which is not `Sync`: run them on a
//! current-thread runtime or in a `tokio::task::LocalSet`. Writes that ask
//! on the terminal (Douyin's `publish` and `edit_post`) take `yes: true` in
//! their request instead; nothing else asks below the CLI.

pub use atp_core::*;

#[cfg(feature = "amap")]
pub use atp_amap as amap;
#[cfg(feature = "baidumap")]
pub use atp_baidumap as baidumap;
#[cfg(feature = "baidupan")]
pub use atp_baidupan as baidupan;
#[cfg(feature = "bangumi")]
pub use atp_bangumi as bangumi;
#[cfg(feature = "bilibili")]
pub use atp_bilibili as bilibili;
#[cfg(feature = "bluesky")]
pub use atp_bluesky as bluesky;
#[cfg(feature = "ctrip")]
pub use atp_ctrip as ctrip;
#[cfg(feature = "dewu")]
pub use atp_dewu as dewu;
#[cfg(feature = "dianping")]
pub use atp_dianping as dianping;
#[cfg(feature = "dingtalk")]
pub use atp_dingtalk as dingtalk;
#[cfg(feature = "discord")]
pub use atp_discord as discord;
#[cfg(feature = "discourse")]
pub use atp_discourse as discourse;
#[cfg(feature = "douban")]
pub use atp_douban as douban;
#[cfg(feature = "douyin")]
pub use atp_douyin as douyin;
#[cfg(feature = "facebook")]
pub use atp_facebook as facebook;
#[cfg(feature = "feishu")]
pub use atp_feishu as feishu;
#[cfg(feature = "googlemaps")]
pub use atp_googlemaps as googlemaps;
#[cfg(feature = "hackernews")]
pub use atp_hackernews as hackernews;
#[cfg(feature = "instagram")]
pub use atp_instagram as instagram;
#[cfg(feature = "jd")]
pub use atp_jd as jd;
#[cfg(feature = "jike")]
pub use atp_jike as jike;
#[cfg(feature = "juejin")]
pub use atp_juejin as juejin;
#[cfg(feature = "kook")]
pub use atp_kook as kook;
#[cfg(feature = "kuaishou")]
pub use atp_kuaishou as kuaishou;
#[cfg(feature = "lemmy")]
pub use atp_lemmy as lemmy;
#[cfg(feature = "line")]
pub use atp_line as line;
#[cfg(feature = "lobsters")]
pub use atp_lobsters as lobsters;
#[cfg(feature = "mastodon")]
pub use atp_mastodon as mastodon;
#[cfg(feature = "matrix")]
pub use atp_matrix as matrix;
#[cfg(feature = "mattermost")]
pub use atp_mattermost as mattermost;
#[cfg(feature = "meituan")]
pub use atp_meituan as meituan;
#[cfg(feature = "misskey")]
pub use atp_misskey as misskey;
#[cfg(feature = "mp")]
pub use atp_mp as mp;
#[cfg(feature = "netease")]
pub use atp_netease as netease;
#[cfg(feature = "onebot")]
pub use atp_onebot as onebot;
#[cfg(feature = "pinduoduo")]
pub use atp_pinduoduo as pinduoduo;
#[cfg(feature = "podcast")]
pub use atp_podcast as podcast;
#[cfg(feature = "qq")]
pub use atp_qq as qq;
#[cfg(feature = "qqmusic")]
pub use atp_qqmusic as qqmusic;
#[cfg(feature = "quark")]
pub use atp_quark as quark;
#[cfg(feature = "reddit")]
pub use atp_reddit as reddit;
#[cfg(feature = "rocketchat")]
pub use atp_rocketchat as rocketchat;
#[cfg(feature = "rss")]
pub use atp_rss as rss;
#[cfg(feature = "satori")]
pub use atp_satori as satori;
#[cfg(feature = "slack")]
pub use atp_slack as slack;
#[cfg(feature = "spotify")]
pub use atp_spotify as spotify;
#[cfg(feature = "stackexchange")]
pub use atp_stackexchange as stackexchange;
#[cfg(feature = "taobao")]
pub use atp_taobao as taobao;
#[cfg(feature = "telegram")]
pub use atp_telegram as telegram;
#[cfg(feature = "tencentmap")]
pub use atp_tencentmap as tencentmap;
#[cfg(feature = "threads")]
pub use atp_threads as threads;
#[cfg(feature = "tieba")]
pub use atp_tieba as tieba;
#[cfg(feature = "tiktok")]
pub use atp_tiktok as tiktok;
#[cfg(feature = "toutiao")]
pub use atp_toutiao as toutiao;
#[cfg(feature = "twitter")]
pub use atp_twitter as twitter;
#[cfg(feature = "v2ex")]
pub use atp_v2ex as v2ex;
#[cfg(feature = "vocechat")]
pub use atp_vocechat as vocechat;
#[cfg(feature = "wechat")]
pub use atp_wechat as wechat;
#[cfg(feature = "wecom")]
pub use atp_wecom as wecom;
#[cfg(feature = "weibo")]
pub use atp_weibo as weibo;
#[cfg(feature = "weread")]
pub use atp_weread as weread;
#[cfg(feature = "whatsapp")]
pub use atp_whatsapp as whatsapp;
#[cfg(feature = "xhs")]
pub use atp_xhs as xhs;
#[cfg(feature = "xianyu")]
pub use atp_xianyu as xianyu;
#[cfg(feature = "xiaoyuzhou")]
pub use atp_xiaoyuzhou as xiaoyuzhou;
#[cfg(feature = "youtube")]
pub use atp_youtube as youtube;
#[cfg(feature = "zhihu")]
pub use atp_zhihu as zhihu;
