//! Feature-flag sets of the operations, as the reference client sends them.

use super::Flags;

/// The reference client's default feature set.
pub(super) const DEFAULT: Flags = &[
  ("responsive_web_graphql_exclude_directive_enabled", true),
  ("verified_phone_label_enabled", false),
  ("creator_subscriptions_tweet_preview_api_enabled", true),
  ("responsive_web_graphql_timeline_navigation_enabled", true),
  (
    "responsive_web_graphql_skip_user_profile_image_extensions_enabled",
    false,
  ),
  ("c9s_tweet_anatomy_moderator_badge_enabled", true),
  ("tweetypie_unmention_optimization_enabled", true),
  ("responsive_web_edit_tweet_api_enabled", true),
  (
    "graphql_is_translatable_rweb_tweet_is_translatable_enabled",
    true,
  ),
  ("view_counts_everywhere_api_enabled", true),
  ("longform_notetweets_consumption_enabled", true),
  (
    "responsive_web_twitter_article_tweet_consumption_enabled",
    true,
  ),
  ("tweet_awards_web_tipping_enabled", false),
  ("longform_notetweets_rich_text_read_enabled", true),
  ("longform_notetweets_inline_media_enabled", true),
  ("rweb_video_timestamps_enabled", true),
  ("responsive_web_media_download_video_enabled", true),
  ("freedom_of_speech_not_reach_fetch_enabled", true),
  ("standardized_nudges_misinfo", true),
  ("responsive_web_enhance_cards_enabled", false),
];

/// Default set plus article previews, for single tweets.
pub(super) const TWEET: Flags = &[
  ("responsive_web_graphql_exclude_directive_enabled", true),
  ("creator_subscriptions_tweet_preview_api_enabled", true),
  ("responsive_web_graphql_timeline_navigation_enabled", true),
  ("c9s_tweet_anatomy_moderator_badge_enabled", true),
  ("tweetypie_unmention_optimization_enabled", true),
  ("responsive_web_edit_tweet_api_enabled", true),
  (
    "graphql_is_translatable_rweb_tweet_is_translatable_enabled",
    true,
  ),
  ("view_counts_everywhere_api_enabled", true),
  ("longform_notetweets_consumption_enabled", true),
  (
    "responsive_web_twitter_article_tweet_consumption_enabled",
    true,
  ),
  ("longform_notetweets_rich_text_read_enabled", true),
  ("longform_notetweets_inline_media_enabled", true),
  ("articles_preview_enabled", true),
  ("rweb_video_timestamps_enabled", true),
  ("freedom_of_speech_not_reach_fetch_enabled", true),
  ("standardized_nudges_misinfo", true),
];

/// The reference client's profile feature set.
pub(super) const USER: Flags = &[
  ("hidden_profile_subscriptions_enabled", true),
  ("rweb_tipjar_consumption_enabled", true),
  ("responsive_web_graphql_exclude_directive_enabled", true),
  ("verified_phone_label_enabled", false),
  (
    "subscriptions_verification_info_is_identity_verified_enabled",
    true,
  ),
  (
    "subscriptions_verification_info_verified_since_enabled",
    true,
  ),
  ("highlights_tweets_tab_ui_enabled", true),
  ("responsive_web_twitter_article_notes_tab_enabled", true),
  ("subscriptions_feature_can_gift_premium", true),
  ("creator_subscriptions_tweet_preview_api_enabled", true),
  (
    "responsive_web_graphql_skip_user_profile_image_extensions_enabled",
    false,
  ),
  ("responsive_web_graphql_timeline_navigation_enabled", true),
];

/// The current web client's set for `CreateTweet` (the reference's default
/// set predates the live query id; mutations reject missing flags).
pub(super) const CREATE: Flags = &[
  ("premium_content_api_read_enabled", false),
  ("communities_web_enable_tweet_community_results_fetch", true),
  ("c9s_tweet_anatomy_moderator_badge_enabled", true),
  (
    "responsive_web_grok_analyze_button_fetch_trends_enabled",
    false,
  ),
  ("responsive_web_grok_analyze_post_followups_enabled", true),
  ("rweb_cashtags_composer_attachment_enabled", true),
  ("responsive_web_jetfuel_frame", true),
  ("responsive_web_grok_share_attachment_enabled", true),
  ("responsive_web_grok_annotations_enabled", true),
  ("responsive_web_edit_tweet_api_enabled", true),
  ("rweb_conversational_replies_downvote_enabled", false),
  (
    "graphql_is_translatable_rweb_tweet_is_translatable_enabled",
    true,
  ),
  ("view_counts_everywhere_api_enabled", true),
  ("longform_notetweets_consumption_enabled", true),
  (
    "responsive_web_twitter_article_tweet_consumption_enabled",
    true,
  ),
  ("content_disclosure_indicator_enabled", true),
  ("content_disclosure_ai_generated_indicator_enabled", true),
  ("responsive_web_grok_show_grok_translated_post", true),
  ("responsive_web_grok_analysis_button_from_backend", true),
  ("post_ctas_fetch_enabled", true),
  ("longform_notetweets_rich_text_read_enabled", true),
  ("longform_notetweets_inline_media_enabled", false),
  ("profile_label_improvements_pcf_label_in_post_enabled", true),
  ("responsive_web_profile_redirect_enabled", false),
  ("rweb_tipjar_consumption_enabled", false),
  ("verified_phone_label_enabled", false),
  ("articles_preview_enabled", true),
  ("rweb_cashtags_enabled", true),
  (
    "responsive_web_grok_community_note_auto_translation_is_enabled",
    true,
  ),
  (
    "responsive_web_graphql_skip_user_profile_image_extensions_enabled",
    false,
  ),
  ("freedom_of_speech_not_reach_fetch_enabled", true),
  ("standardized_nudges_misinfo", true),
  (
    "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled",
    true,
  ),
  ("responsive_web_grok_image_annotation_enabled", true),
  ("responsive_web_grok_imagine_annotation_enabled", true),
  ("responsive_web_graphql_timeline_navigation_enabled", true),
];

pub(super) const NONE: Flags = &[];

/// The live web client's set for timelines added later (`UserTweets` and
/// friends as of 2026-09, from fa0311/TwitterInternalAPIDocument).
pub(super) const CURRENT: Flags = &[
  ("rweb_video_screen_enabled", false),
  ("rweb_cashtags_enabled", true),
  ("profile_label_improvements_pcf_label_in_post_enabled", true),
  ("responsive_web_profile_redirect_enabled", true),
  ("rweb_tipjar_consumption_enabled", false),
  ("verified_phone_label_enabled", false),
  ("creator_subscriptions_tweet_preview_api_enabled", true),
  ("responsive_web_graphql_timeline_navigation_enabled", true),
  ("premium_content_api_read_enabled", false),
  ("communities_web_enable_tweet_community_results_fetch", true),
  ("c9s_tweet_anatomy_moderator_badge_enabled", true),
  (
    "responsive_web_grok_analyze_button_fetch_trends_enabled",
    false,
  ),
  ("responsive_web_grok_analyze_post_followups_enabled", false),
  ("rweb_cashtags_composer_attachment_enabled", true),
  ("responsive_web_jetfuel_frame", true),
  ("rweb_sports_post_context_enabled", true),
  ("responsive_web_grok_share_attachment_enabled", true),
  ("responsive_web_grok_annotations_enabled", true),
  ("articles_preview_enabled", true),
  ("responsive_web_edit_tweet_api_enabled", true),
  (
    "graphql_is_translatable_rweb_tweet_is_translatable_enabled",
    true,
  ),
  ("view_counts_everywhere_api_enabled", true),
  ("longform_notetweets_consumption_enabled", true),
  (
    "responsive_web_twitter_article_tweet_consumption_enabled",
    true,
  ),
  ("content_disclosure_indicator_enabled", true),
  ("content_disclosure_ai_generated_indicator_enabled", true),
  ("responsive_web_grok_show_grok_translated_post", true),
  ("responsive_web_grok_analysis_button_from_backend", true),
  ("post_ctas_fetch_enabled", false),
  ("freedom_of_speech_not_reach_fetch_enabled", true),
  ("standardized_nudges_misinfo", true),
  (
    "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled",
    true,
  ),
  ("longform_notetweets_rich_text_read_enabled", true),
  ("longform_notetweets_inline_media_enabled", false),
  ("responsive_web_nested_quote_preview_enabled", false),
  ("responsive_web_grok_image_annotation_enabled", true),
  ("responsive_web_grok_imagine_annotation_enabled", true),
  (
    "responsive_web_grok_community_note_auto_translation_is_enabled",
    true,
  ),
  ("responsive_web_enhance_cards_enabled", false),
];

/// Lists and communities as entities (and their mutations).
pub(super) const ENTITY: Flags = &[
  ("profile_label_improvements_pcf_label_in_post_enabled", true),
  ("responsive_web_profile_redirect_enabled", true),
  ("rweb_tipjar_consumption_enabled", false),
  ("verified_phone_label_enabled", false),
  ("responsive_web_graphql_timeline_navigation_enabled", true),
];

/// `AudioSpaceById`: the current set plus the Spaces flags.
pub(super) const SPACE: Flags = &[
  ("spaces_2022_h2_spaces_communities", true),
  ("spaces_2022_h2_clipping", true),
  ("creator_subscriptions_tweet_preview_api_enabled", true),
  ("profile_label_improvements_pcf_label_in_post_enabled", true),
  ("responsive_web_profile_redirect_enabled", true),
  ("rweb_tipjar_consumption_enabled", false),
  ("verified_phone_label_enabled", false),
  ("premium_content_api_read_enabled", false),
  ("communities_web_enable_tweet_community_results_fetch", true),
  ("c9s_tweet_anatomy_moderator_badge_enabled", true),
  ("responsive_web_edit_tweet_api_enabled", true),
  (
    "graphql_is_translatable_rweb_tweet_is_translatable_enabled",
    true,
  ),
  ("view_counts_everywhere_api_enabled", true),
  ("longform_notetweets_consumption_enabled", true),
  (
    "responsive_web_twitter_article_tweet_consumption_enabled",
    true,
  ),
  ("articles_preview_enabled", true),
  ("freedom_of_speech_not_reach_fetch_enabled", true),
  ("standardized_nudges_misinfo", true),
  (
    "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled",
    true,
  ),
  ("longform_notetweets_rich_text_read_enabled", true),
  ("longform_notetweets_inline_media_enabled", false),
  ("responsive_web_graphql_timeline_navigation_enabled", true),
];
