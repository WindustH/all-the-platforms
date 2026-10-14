//! Lists as tables: posts, users, collections, notifications.

use super::*;

pub(super) fn post_table(posts: &[Post]) {
  if is_places(posts) {
    return place_table(posts);
  }
  // Counts chat messages lack stay out.
  let likes = posts.iter().any(|p| p.metrics.likes.is_some());
  let comments = posts.iter().any(|p| p.metrics.comments.is_some());
  let views = posts.iter().any(|p| p.metrics.views.is_some());
  let prices = posts.iter().any(|p| p.price.is_some());
  let mut header = vec!["#", "Content", "Author"];
  if prices {
    header.push("Price");
  }
  if likes {
    header.push("Likes");
  }
  if comments {
    header.push("Cmts");
  }
  if views {
    header.push("Views");
  }
  header.extend(["Time", "ID"]);
  let mut t = table(&header);
  for (i, p) in posts.iter().enumerate() {
    let mut row = vec![
      index(i),
      Cell::new(headline(p)),
      Cell::new(
        p.author
          .as_ref()
          .map(|a| truncate(&a.name, 16))
          .unwrap_or_default(),
      ),
    ];
    if prices {
      let price = p.price.as_ref().map(Price::display).unwrap_or_default();
      row.push(Cell::new(price).set_alignment(CellAlignment::Right));
    }
    if likes {
      row.push(num(p.metrics.likes));
    }
    if comments {
      row.push(num(p.metrics.comments));
    }
    if views {
      row.push(num(p.metrics.views));
    }
    row.push(Cell::new(p.created_at.map(fmt_time).unwrap_or_default()));
    row.push(Cell::new(&p.id).add_attribute(Attribute::Dim));
    t.add_row(row);
  }
  println!("{t}");
}

pub(super) fn user_table(users: &[User]) {
  let mut t = table(&["#", "Name", "Followers", "Posts", "Bio", "ID"]);
  for (i, u) in users.iter().enumerate() {
    t.add_row(vec![
      index(i),
      Cell::new(truncate(&user_label(u), 32)),
      num(u.stats.followers),
      num(u.stats.posts),
      Cell::new(truncate(&one_line(u.bio.as_deref().unwrap_or("")), 48)),
      Cell::new(&u.id).add_attribute(Attribute::Dim),
    ]);
  }
  println!("{t}");
}

pub(super) fn collection_table(items: &[Collection]) {
  // Counts most kinds lack (activities, folders of some platforms) stay out.
  let items_col = items.iter().any(|c| c.items.is_some());
  let followers_col = items.iter().any(|c| c.followers.is_some());
  let mut header = vec!["#", "Name", "Kind"];
  header.extend(items_col.then_some("Items"));
  header.extend(followers_col.then_some("Followers"));
  header.extend(["Description", "ID"]);
  let mut t = table(&header);
  for (i, c) in items.iter().enumerate() {
    let mut row = vec![
      index(i),
      Cell::new(truncate(&c.name, 32)),
      Cell::new(&c.kind),
    ];
    row.extend(items_col.then(|| num(c.items)));
    row.extend(followers_col.then(|| num(c.followers)));
    row.extend([
      Cell::new(truncate(
        &one_line(c.description.as_deref().unwrap_or("")),
        40,
      )),
      Cell::new(&c.id).add_attribute(Attribute::Dim),
    ]);
    t.add_row(row);
  }
  println!("{t}");
}

pub(super) fn notification_table(items: &[Notification]) {
  let mut t = table(&["#", "Kind", "From", "Text", "Time"]);
  for (i, n) in items.iter().enumerate() {
    let mut text = one_line(&n.text);
    if let Some(target) = &n.target {
      text = format!("{text} · {}", one_line(target));
    }
    let kind = if n.unread == Some(true) {
      format!("{} •", n.kind)
    } else {
      n.kind.clone()
    };
    t.add_row(vec![
      index(i),
      Cell::new(kind),
      Cell::new(
        n.actor
          .as_ref()
          .map(|a| truncate(&a.name, 16))
          .unwrap_or_default(),
      ),
      Cell::new(truncate(&text, 72)),
      Cell::new(n.created_at.map(fmt_time).unwrap_or_default()),
    ]);
  }
  println!("{t}");
}
