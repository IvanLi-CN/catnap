# DB Contracts

## New table: `user_config_archives`

```sql
CREATE TABLE user_config_archives (
  user_id TEXT NOT NULL,
  config_id TEXT NOT NULL,
  cleaned_at TEXT NOT NULL,
  PRIMARY KEY (user_id, config_id)
);
CREATE INDEX idx_user_config_archives_user_cleaned_at
  ON user_config_archives (user_id, cleaned_at DESC);
CREATE INDEX idx_user_config_archives_config_id
  ON user_config_archives (config_id);
```

## Read path changes

When listing configs for a specific user, join by `(user_id, config_id)`:

- `cleanup_at = user_config_archives.cleaned_at` (nullable)

## Relist cleanup rule

When a config is upserted as `active` (initial fetch / refresh apply):

- delete from `user_config_archives` where `config_id` is in the active upsert set.
- this clears stale user archive markers after relist.
