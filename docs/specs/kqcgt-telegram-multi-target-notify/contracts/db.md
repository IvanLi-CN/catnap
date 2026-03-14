# DB Contract

## settings

Add column:
- `telegram_targets_json` TEXT NULL

Meaning:
- JSON array string of normalized Telegram targets.
- Main source of truth for Telegram multi-target settings.
- Legacy `telegram_target` remains for compatibility and mirrors the first target when available.

Migration/backfill rules:
- if `telegram_targets_json` is missing, add it as nullable;
- when reading, if `telegram_targets_json` is null/empty/invalid and legacy `telegram_target` is non-empty, expose `[telegram_target]`;
- implementation may lazily rewrite normalized JSON on the next successful settings save.

## notification_record_deliveries

New table:

- `id` TEXT PRIMARY KEY
- `record_id` TEXT NOT NULL
- `channel` TEXT NOT NULL (`telegram|webPush`)
- `target` TEXT NULL
- `status` TEXT NOT NULL (`success|partial_success|error|skipped|pending|not_sent` as applicable per row)
- `error_message` TEXT NULL
- `created_at` TEXT NOT NULL
- `updated_at` TEXT NOT NULL

Indexes:
- `(record_id, channel, created_at, id)`
- `(channel, created_at DESC)` optional for diagnostics

Rules:
- Telegram fan-out inserts one row per attempted target;
- Web Push may keep using only aggregate status for now; table can remain Telegram-only in practice but `channel` stays generic;
- deleting a notification record must also delete its delivery rows.
