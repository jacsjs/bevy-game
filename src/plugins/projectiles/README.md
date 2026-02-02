# Projectiles v3 (Message-based spawn requests)

This patch implements a producer → consumer spawn pipeline using Bevy **Messages**:

- Producers write `SpawnBulletRequest` using `MessageWriter<T>`.
- The consumer reads requests using `MessageReader<T>` and activates bullets from `BulletPool`.
- The message buffer is maintained by calling `Messages<T>::update()` once per frame.

Why Messages?

- Bevy 0.18 supports buffered **Messages** and provides `MessageWriter` / `MessageReader` system params. citeturn43search40turn43search30
- Messages are evaluated at fixed points in the schedule (predictable), and are double-buffered via `Messages::update`. citeturn43search40turn43search35

Notes:

- MessageWriter/Reader are still backed by a resource (`Messages<T>`), so multiple writers of the same message type will not run concurrently. citeturn43search30turn43search40
- This patch avoids `App::add_event` / `.in_set` entirely and sticks to `.after` ordering.
