use std::process::ExitCode;

fn main() -> ExitCode {
  social_core::App::new(
    "social",
    env!("CARGO_PKG_VERSION"),
    env!("CARGO_PKG_DESCRIPTION"),
  )
  .platform::<social_zhihu::Zhihu>()
  .platform::<social_xhs::Xhs>()
  .platform::<social_twitter::Twitter>()
  .platform::<social_bilibili::Bilibili>()
  .platform::<social_reddit::Reddit>()
  .platform::<social_youtube::YouTube>()
  .platform::<social_mp::Mp>()
  .platform::<social_telegram::Telegram>()
  .platform::<social_weibo::Weibo>()
  .platform::<social_douyin::Douyin>()
  .platform::<social_discord::Discord>()
  .platform::<social_tieba::Tieba>()
  .platform::<social_tiktok::TikTok>()
  .platform::<social_wechat::Wechat>()
  .platform::<social_feishu::Feishu>()
  .platform::<social_dingtalk::DingTalk>()
  .platform::<social_wecom::WeCom>()
  .run()
}
