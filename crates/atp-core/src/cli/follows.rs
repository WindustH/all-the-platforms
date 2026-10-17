//! Managing whom you follow and who follows you: several users at once,
//! followers, blocks and follow groups.

use clap::Subcommand;

use super::args::PageArgs;
use super::input::confirm;
use crate::error::Result;
use crate::model::Data;
use crate::platform::Platform;

#[derive(Debug, Subcommand)]
pub enum FollowCommand {
  /// Make users stop following you
  #[command(after_long_help = EX_REMOVE_FOLLOWER)]
  RemoveFollower {
    /// Users: ids, handles or links, or #N from the last list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Block users
  #[command(after_long_help = EX_BLOCK)]
  Block {
    /// Users: ids, handles or links, or #N from the last list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
    /// Unblock instead
    #[arg(long)]
    undo: bool,
  },
  /// Unblock users
  Unblock {
    /// Users: ids, handles or links, or #N from the last list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
  },
  /// The accounts you block
  Blocked {
    #[command(flatten)]
    page: PageArgs,
  },
  #[command(flatten)]
  Groups(GroupCommand),
}

/// Follow groups (关注分组): lists of the people you follow.
#[derive(Debug, Subcommand)]
pub enum GroupCommand {
  /// Your follow groups (lists you sort the people you follow into)
  #[command(after_long_help = EX_FOLLOW_GROUPS)]
  FollowGroups {
    #[command(flatten)]
    page: PageArgs,
  },
  /// The people in one of your follow groups
  FollowGroup {
    /// Follow group: its id, or #N after `follow-groups`
    #[arg(allow_negative_numbers = true)]
    group: String,
    /// Order (values differ per platform)
    #[arg(short, long)]
    sort: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a follow group
  FollowGroupCreate {
    /// Name of the group
    name: String,
  },
  /// Rename a follow group
  FollowGroupRename {
    /// Follow group: its id, or #N after `follow-groups`
    #[arg(allow_negative_numbers = true)]
    group: String,
    /// New name
    name: String,
  },
  /// Delete a follow group (its people stay followed)
  FollowGroupDelete {
    /// Follow group: its id, or #N after `follow-groups`
    #[arg(allow_negative_numbers = true)]
    group: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Put users into a follow group (their other groups stay)
  #[command(after_long_help = EX_FOLLOW_GROUP_ADD)]
  FollowGroupAdd {
    /// Follow group: its id, or #N after `follow-groups`
    #[arg(allow_negative_numbers = true)]
    group: String,
    /// Users: ids, handles or links, or #N from the last user list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
  },
  /// Take users out of a follow group (they stay followed)
  FollowGroupRemove {
    /// Follow group: its id, or #N after `follow-groups`
    #[arg(allow_negative_numbers = true)]
    group: String,
    /// Users: ids, handles or links, or #N from the last user list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
  },
}

fn user_refs<P: Platform>(p: &P, list: &[String]) -> Result<Vec<String>> {
  list.iter().map(|u| p.ctx().user_ref(u)).collect()
}

pub(super) async fn run<P: Platform>(p: &P, cmd: FollowCommand) -> Result<Data> {
  let users = |list: &[String]| user_refs(p, list);
  Ok(match cmd {
    FollowCommand::RemoveFollower { users: list, yes } => {
      let list = users(&list)?;
      confirm(yes, &format!("remove {}", several("follower", &list)))?;
      Data::actions(p.remove_followers(&list).await?)
    }
    FollowCommand::Block { users: list, undo } => {
      Data::actions(p.block(&users(&list)?, undo).await?)
    }
    FollowCommand::Unblock { users: list } => Data::actions(p.block(&users(&list)?, true).await?),
    FollowCommand::Blocked { page } => {
      Data::Users(page.collect(async |r| p.blocked(&r).await).await?)
    }
    FollowCommand::Groups(cmd) => return groups(p, cmd).await,
  })
}

async fn groups<P: Platform>(p: &P, cmd: GroupCommand) -> Result<Data> {
  let group = |g: &str| p.ctx().collection_ref(g);
  Ok(match cmd {
    GroupCommand::FollowGroups { page } => {
      Data::Collections(page.collect(async |r| p.follow_groups(&r).await).await?)
    }
    GroupCommand::FollowGroup {
      group: g,
      sort,
      page,
    } => {
      let (g, sort) = (group(&g)?, sort.as_deref());
      Data::Users(
        page
          .collect(async |r| p.follow_group(&g, sort, &r).await)
          .await?,
      )
    }
    GroupCommand::FollowGroupCreate { name } => Data::Action(p.follow_group_create(&name).await?),
    GroupCommand::FollowGroupRename { group: g, name } => {
      Data::Action(p.follow_group_rename(&group(&g)?, &name).await?)
    }
    GroupCommand::FollowGroupDelete { group: g, yes } => {
      let g = group(&g)?;
      confirm(yes, &format!("delete follow group {g}"))?;
      Data::Action(p.follow_group_delete(&g).await?)
    }
    GroupCommand::FollowGroupAdd { group: g, users } => {
      let users = user_refs(p, &users)?;
      Data::actions(p.follow_group_add(&group(&g)?, &users).await?)
    }
    GroupCommand::FollowGroupRemove { group: g, users } => {
      let users = user_refs(p, &users)?;
      Data::actions(p.follow_group_remove(&group(&g)?, &users).await?)
    }
  })
}

/// `follow` / `unfollow`: one user exactly as before, several through
/// the platform's batch hook.
pub(super) async fn follow<P: Platform>(p: &P, users: &[String], undo: bool) -> Result<Data> {
  let users = users
    .iter()
    .map(|u| p.follow_ref(u))
    .collect::<Result<Vec<_>>>()?;
  if let [user] = users.as_slice() {
    return Ok(Data::Action(p.follow(user, undo).await?));
  }
  Ok(Data::Actions(p.follow_users(&users, undo).await?))
}

/// `follower 123`, or `3 followers (1, 2, 3)`: what a confirmation names.
pub(super) fn several(noun: &str, items: &[String]) -> String {
  match items {
    [one] => format!("{noun} {one}"),
    _ => format!("{} {noun}s ({})", items.len(), items.join(", ")),
  }
}

const EX_REMOVE_FOLLOWER: &str = "\
Examples:
  atp bili followers -n 50
  atp bili remove-follower '#3' '#7'
  atp bili remove-follower 946974 -y

They are not blocked: they may follow you again.
More: atp guide <platform> remove-follower";

const EX_BLOCK: &str = "\
Examples:
  atp bili block 946974
  atp bili block '#2' '#4' '#9'
  atp bili unblock 946974
  atp bili blocked

More: atp guide <platform> block";

const EX_FOLLOW_GROUPS: &str = "\
Examples:
  atp bili follow-groups
  atp bili follow-group '#2' -n 100
  atp bili follow-group-create 'Game streams'
  atp bili follow-group-rename '#3' 'Streams'

More: atp guide <platform> follow-groups";

const EX_FOLLOW_GROUP_ADD: &str = "\
Examples:
  atp bili follow-groups
  atp bili following -n 50
  atp bili follow-group-add '#2' 946974 '#5'
  atp bili follow-group-remove 194111 946974

The group comes from the last collection list (`follow-groups`), the
users from the last user list. More: atp guide <platform> follow-groups";
