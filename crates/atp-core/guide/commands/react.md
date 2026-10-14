Adds an emoji reaction to a message or post, or with `--undo` removes
yours.

    atp tg react '#2' 👍
    atp discord react 123456/789012 🎉 --undo
    atp slack react '#1' rocket

- POST: `CHAT/MESSAGE`, a link, or `#N` of the last message or post list.
- EMOJI: the emoji itself. Platforms that name their reactions (Slack,
  Mattermost, Rocket.Chat) take the name with or without colons, and
  common emoji are translated (`👍` becomes `+1`). Custom emoji forms are in
  the platform's notes.
- The result is an Action.

More: `atp guide refs`
