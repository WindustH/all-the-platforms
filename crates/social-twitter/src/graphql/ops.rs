//! The operations, with fallback query ids.

use super::Op;
use super::features::*;

// ── reading ───────────────────────────────────────────────────────────

pub const USER_BY_SCREEN_NAME: Op =
  Op::get("UserByScreenName", "KybxDj9RrADIITXlGG8kpw", USER).guest();
pub const USER_BY_REST_ID: Op = Op::get("UserByRestId", "IdmRdjYxIGI39Hdwkwo5cQ", USER).guest();
pub const TWEET_RESULT: Op = Op::get("TweetResultByRestId", "LbQZrAWyKPvExi8di3-EoA", TWEET)
  .toggles(&[
    ("withArticleRichContentState", false),
    ("withArticlePlainText", true),
  ])
  .guest();
pub const TWEET_DETAIL: Op = Op::get("TweetDetail", "blErEeZkos5TDrWmrCp7cw", DEFAULT).toggles(&[
  ("withArticleRichContentState", true),
  ("withArticlePlainText", false),
  ("withGrokAnalyze", false),
  ("withDisallowedReplyControls", false),
]);
pub const USER_TWEETS: Op = Op::get("UserTweets", "qJy3MbaNndtzxf9IqUzxMg", DEFAULT).guest();
pub const LIKES: Op = Op::get("Likes", "PgAssYGsPMMF1vVox5ysPg", DEFAULT);
pub const SEARCH: Op = Op::get("SearchTimeline", "uGB-gNd5HE4TkpO70OcFNw", DEFAULT);
pub const FOLLOWERS: Op = Op::get("Followers", "mrqxgX8JzwlL6pvYiC5CPA", DEFAULT);
pub const FOLLOWING: Op = Op::get("Following", "uwmIAx89XrXNuGY-Y7WFLg", DEFAULT);
pub const HOME: Op = Op::get("HomeTimeline", "7zlnp2TxC044W4C1ZUJMHw", DEFAULT);
pub const HOME_LATEST: Op = Op::get("HomeLatestTimeline", "0dateTVgvXjpkf7kyBZy0g", DEFAULT);
pub const BOOKMARKS: Op = Op::get("Bookmarks", "XD0ViOeSOW4YoeNTGjVaYw", DEFAULT);
pub const BOOKMARK_FOLDERS: Op = Op::get("BookmarkFoldersSlice", "i78YDd0Tza-dV4SYs58kRg", DEFAULT);
pub const BOOKMARK_FOLDER: Op =
  Op::get("BookmarkFolderTimeline", "hNY7X2xE2N7HVF6Qb_mu6w", DEFAULT);
pub const LIST_TWEETS: Op = Op::get(
  "ListLatestTweetsTimeline",
  "FVWmROVvhgjRPC-4jAUh8A",
  DEFAULT,
);
pub const LIST_OWNERSHIPS: Op = Op::get("ListOwnerships", "5eUATiy7RZHeOMVM5ZZIcg", DEFAULT);
pub const EXPLORE_TIMELINE: Op = Op::get("GenericTimelineById", "S_hzVUv1trgZ_5ruDe2IoA", DEFAULT);
pub const NOTIFICATIONS: Op = Op::get("NotificationsTimeline", "gzC0OYBCnfdYS4M4Gue7BA", DEFAULT);
// Lazy chunk `shared~bundle.QuoteTweetActivity~bundle.TweetActivity` (the "post engagements" screen).
pub const FAVORITERS: Op = Op::get("Favoriters", "HaFAhly6sDpoeqGEbFb2Ig", DEFAULT);
pub const RETWEETERS: Op = Op::get("Retweeters", "UBCF0EF800cPqREAeu1uuA", DEFAULT);

// Profile tabs, people and similar posts (query ids of 2026-09).
pub const USER_REPLIES: Op = Op::get("UserTweetsAndReplies", "wI-ubAWfScnG6odLK4XgCg", CURRENT);
pub const USER_MEDIA: Op = Op::get("UserMedia", "TwLiEVUhRjjjKVc98IT0TQ", CURRENT);
pub const USER_HIGHLIGHTS: Op = Op::get("UserHighlightsTweets", "TtovDhtuqVtzzfZ9fpztlw", CURRENT);
pub const USER_ARTICLES: Op = Op::get("UserArticlesTweets", "ic9_Nc9wg4whcOEBiWAP4w", CURRENT);
pub const SIMILAR_POSTS: Op = Op::get("SimilarPosts", "pnl_SipPG1I4d2CBGCEgQw", CURRENT);
pub const VERIFIED_FOLLOWERS: Op =
  Op::get("BlueVerifiedFollowers", "DWeIe6l1rsZMqHPbbWXtig", CURRENT);
pub const FOLLOWERS_YOU_KNOW: Op = Op::get("FollowersYouKnow", "hEiuVHcQAxCsfSE9aL2e_g", CURRENT);
pub const EDIT_HISTORY: Op = Op::get("TweetEditHistory", "ov6xadTBRl1SNc7zOkBYNg", CURRENT);

// Lists.
pub const LIST_BY_ID: Op = Op::get("ListByRestId", "EAARFZGlY-JHdLJbKZAA5g", ENTITY);
pub const LIST_MEMBERS: Op = Op::get("ListMembers", "QUEL7b9q4yBmJ7AP2u5Jtg", CURRENT);
pub const LIST_SUBSCRIBERS: Op = Op::get("ListSubscribers", "U1R6slIDy8ufW8qv3Pio0A", CURRENT);
pub const LIST_MEMBERSHIPS: Op = Op::get("ListMemberships", "dBtqkQ7YcJqQ2-Yb-itgzg", CURRENT);

// Communities.
pub const COMMUNITY: Op = Op::get("CommunityByRestId", "kbGfpn9QxGj0vp65v6FfKA", ENTITY);
pub const COMMUNITY_TWEETS: Op =
  Op::get("CommunityTweetsTimeline", "VgO2y9wXrpUPlzZmfHtKKA", CURRENT);
pub const COMMUNITY_MEDIA: Op =
  Op::get("CommunityMediaTimeline", "9a4dhEcbxvATiUemDoRhCA", CURRENT);
pub const COMMUNITY_MEMBERS: Op = Op::get(
  "membersSliceTimeline_Query",
  "KDAssJ5lafCy-asH4wm1dw",
  &[("responsive_web_graphql_timeline_navigation_enabled", true)],
);
pub const COMMUNITY_MODERATORS: Op = Op::get(
  "moderatorsSliceTimeline_Query",
  "9KI_r8e-tgp3--N5SZYVjg",
  &[("responsive_web_graphql_timeline_navigation_enabled", true)],
);
pub const COMMUNITY_MEMBERSHIPS: Op = Op::get(
  "CommunitiesMembershipsSlice",
  "njA6yQGGzMux6jgelblxNw",
  ENTITY,
);

// Spaces.
pub const SPACE_BY_ID: Op = Op::get("AudioSpaceById", "ipEqgz4P_bZCGY3fBnfsxQ", SPACE);

// Your account.
pub const BLOCKED: Op = Op::get("BlockedAccountsAll", "VlsRzrlUBsLKHvQdoYdSGw", CURRENT);
pub const MUTED: Op = Op::get("MutedAccounts", "doSy_eEELN6ol2addK5OfA", CURRENT);
pub const SCHEDULED: Op = Op::get("FetchScheduledTweets", "H2elmT2R9DLhWoo0DZFNkA", NONE);

// ── analytics: Relay queries of x.com/i/account_analytics (`bundle.AccountAnalytics`) ──

/// Rollup of a few metrics shown to every account (the analytics upsell).
pub const FREE_ROLLUP: Op = Op::get("useFetchAnalyticsQuery", "5JkoDLRvQrXv2QV4U5gKFg", NONE);
/// Account metrics per day (`organic_metrics_time_series`).
pub const ACCOUNT_SERIES: Op = Op::get("overviewDataUserQuery", "NlJ6RM-hgHxt-iu9cPQz7A", NONE);
/// Post metrics per day.
pub const POST_SERIES: Op = Op::get("overviewDataPostQuery", "9c83mWUXFc4RuVLInF9SOQ", NONE);
/// Post metrics since publication (`organic_metrics_total`).
pub const POST_TOTALS: Op = Op::get(
  "postDetailsProviderMetricsTotalQuery",
  "yLIUkOUqs-4MT8I5gUyztQ",
  NONE,
);
/// Audience of the account: engagements by age, gender, app, network, country, hour.
pub const ACCOUNT_AUDIENCE: Op =
  Op::get("audienceOverviewDataQuery", "H47r_cVD9Uu-qMQLktBCKA", NONE);
pub const POST_AUDIENCE: Op = Op::get(
  "postDetailsProviderAudienceQuery",
  "S4-UXaX7xV7kLLelCMej3g",
  NONE,
);
/// The account's posts of a period with their metrics (the "Content" tab).
pub const CONTENT: Op = Op::get(
  "ContentPageV2UserTweetsQuery",
  "7uyOLS6aSCF-HaYHhaZXhw",
  NONE,
);

// ── writing ───────────────────────────────────────────────────────────

pub const CREATE_TWEET: Op = Op::post("CreateTweet", "WNkbkQ_JLIofjdukTXahVA", CREATE);
pub const DELETE_TWEET: Op = Op::post("DeleteTweet", "nxpZCY2K-I6QoFHAHeojFQ", NONE);
pub const FAVORITE: Op = Op::post("FavoriteTweet", "lI07N6Otwv1PhnEgXILM7A", NONE);
pub const UNFAVORITE: Op = Op::post("UnfavoriteTweet", "ZYKSe-w7KEslx3JhSIk5LA", NONE);
pub const RETWEET: Op = Op::post("CreateRetweet", "mbRO74GrOvSfRcJnlMapnQ", NONE);
pub const UNRETWEET: Op = Op::post("DeleteRetweet", "ZyZigVsNiFO6v1dEks1eWg", NONE);
pub const BOOKMARK: Op = Op::post("CreateBookmark", "aoDbu3RHznuiSkQ9aNM67Q", NONE);
pub const UNBOOKMARK: Op = Op::post("DeleteBookmark", "Wlmlj2-xzyS1GN3a6cj-mQ", NONE);
pub const PIN_TWEET: Op = Op::post("PinTweet", "VIHsNu89pK-kW35JpHq7Xw", NONE);
pub const UNPIN_TWEET: Op = Op::post("UnpinTweet", "BhKei844ypCyLYCg0nwigw", NONE);
pub const CREATE_SCHEDULED: Op = Op::post("CreateScheduledTweet", "LCVzRQGxOaGnOnYH01NQXg", NONE);
pub const DELETE_SCHEDULED: Op = Op::post("DeleteScheduledTweet", "CTOVqej0JBXAZSwkp1US0g", NONE);
pub const CREATE_LIST: Op = Op::post("CreateList", "AkWrYT3WjoBVkzbnbvLkhg", ENTITY);
pub const UPDATE_LIST: Op = Op::post("UpdateList", "6fJbXehrO7k4iSr_TK1U2Q", ENTITY);
pub const DELETE_LIST: Op = Op::post("DeleteList", "UnN9Th1BDbeLjpgjGSpL3Q", NONE);
pub const LIST_ADD: Op = Op::post("ListAddMember", "F4BvT6Af48GSxTgqNLIdrQ", ENTITY);
pub const LIST_REMOVE: Op = Op::post("ListRemoveMember", "llA1p2EP5J3gReAQQsW1vw", ENTITY);
pub const LIST_SUBSCRIBE: Op = Op::post("ListSubscribe", "1B3FqCK_7uU6W_AHqWBx8A", ENTITY);
pub const LIST_UNSUBSCRIBE: Op = Op::post("ListUnsubscribe", "8gopUa1KU_9afKsxI9Y_Rg", ENTITY);
pub const JOIN_COMMUNITY: Op = Op::post("JoinCommunity", "tas06cdyWKlygZ2ldJ5B4A", ENTITY);
pub const LEAVE_COMMUNITY: Op = Op::post("LeaveCommunity", "9F6Fdu6ibMAKEpb9nFjUdQ", ENTITY);
pub const FOLDER_ADD: Op = Op::post("bookmarkTweetToFolder", "4KHZvvNbHNf07bsgnL9gWA", NONE);
pub const FOLDER_REMOVE: Op = Op::post(
  "RemoveTweetFromBookmarkFolder",
  "2Qbj9XZvtUvyJB4gFwWfaA",
  NONE,
);
pub const FOLDER_CREATE: Op = Op::post("createBookmarkFolder", "6Xxqpq8TM_CREYiuof_h5w", NONE);
pub const FOLDER_RENAME: Op = Op::post("EditBookmarkFolder", "a6kPp1cS1Dgbsjhapz1PNw", NONE);
pub const FOLDER_DELETE: Op = Op::post("DeleteBookmarkFolder", "2UTTsO-6zs93XqlEUZPsSg", NONE);
// Direct messages (ids from twikit; refreshed from the live client when they rotate).
pub const DM_REACT: Op = Op::post(
  "useDMReactionMutationAddMutation",
  "VyDyV9pC2oZEj6g52hgnhA",
  NONE,
);
pub const DM_UNREACT: Op = Op::post(
  "useDMReactionMutationRemoveMutation",
  "bV_Nim3RYHsaJwMkTXJ6ew",
  NONE,
);
pub const DM_DELETE: Op = Op::post("DMMessageDeleteMutation", "BJ6DtxA2llfjnRoRjaiIiw", NONE);
