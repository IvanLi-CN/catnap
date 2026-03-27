# DB Contract

## lazycat_accounts

New table:

- `user_id` TEXT PRIMARY KEY
- `email` TEXT NOT NULL
- `password` TEXT NOT NULL
- `cookies_json` TEXT NULL
- `state` TEXT NOT NULL
- `last_error` TEXT NULL
- `last_authenticated_at` TEXT NULL
- `last_site_sync_at` TEXT NULL
- `last_panel_sync_at` TEXT NULL
- `created_at` TEXT NOT NULL
- `updated_at` TEXT NOT NULL

Rules:

- at most one row per Catnap user;
- cookies are serialized session cookies for `lxc.lazycat.wiki`;
- deleting the account row is treated as “disconnect account”.

## lazycat_machines

New table:

- `user_id` TEXT NOT NULL
- `service_id` INTEGER NOT NULL
- `service_name` TEXT NOT NULL
- `service_code` TEXT NOT NULL
- `status` TEXT NOT NULL
- `os` TEXT NULL
- `primary_address` TEXT NULL
- `extra_addresses_json` TEXT NOT NULL
- `billing_cycle` TEXT NULL
- `renew_price` TEXT NULL
- `first_price` TEXT NULL
- `expires_at` TEXT NULL
- `panel_kind` TEXT NULL
- `panel_url` TEXT NULL
- `panel_hash` TEXT NULL
- `traffic_used_gb` REAL NULL
- `traffic_limit_gb` REAL NULL
- `traffic_reset_day` INTEGER NULL
- `traffic_last_reset_at` TEXT NULL
- `traffic_display` TEXT NULL
- `last_site_sync_at` TEXT NULL
- `last_panel_sync_at` TEXT NULL
- `detail_state` TEXT NOT NULL
- `detail_error` TEXT NULL
- `created_at` TEXT NOT NULL
- `updated_at` TEXT NOT NULL

Primary key:

- `(user_id, service_id)`

Rules:

- main-site sync overwrites core machine fields but must preserve last-good panel detail when a panel refresh fails;
- a main-site discovery result with zero parsed machines is treated as a failed refresh, not authoritative emptiness, and must not delete existing rows;
- `extra_addresses_json` stores normalized `assignedips` / extra address list;
- `panel_hash` is cached only for the specific machine returned by lazycat main site.

## lazycat_port_mappings

New table:

- `user_id` TEXT NOT NULL
- `service_id` INTEGER NOT NULL
- `family` TEXT NOT NULL (`v4|v6|nat`)
- `mapping_key` TEXT NOT NULL
- `public_ip` TEXT NULL
- `public_port` INTEGER NULL
- `public_port_end` INTEGER NULL
- `private_ip` TEXT NULL
- `private_port` INTEGER NULL
- `private_port_end` INTEGER NULL
- `protocol` TEXT NULL
- `status` TEXT NULL
- `description` TEXT NULL
- `remote_created_at` TEXT NULL
- `remote_updated_at` TEXT NULL
- `sync_at` TEXT NOT NULL

Primary key:

- `(user_id, service_id, family, mapping_key)`

Rules:

- successful panel/NAT sync replaces the mapping set for that machine + family;
- failed panel/NAT sync must not eagerly delete the existing rows;
- failed main-site discovery must not eagerly delete the existing rows;
- disconnecting an account deletes all cached mappings for that user.

## lazycat_traffic_samples

New table:

- `user_id` TEXT NOT NULL
- `service_id` INTEGER NOT NULL
- `bucket_at` TEXT NOT NULL
- `sampled_at` TEXT NOT NULL
- `cycle_start_at` TEXT NOT NULL
- `cycle_end_at` TEXT NOT NULL
- `used_gb` REAL NOT NULL
- `limit_gb` REAL NOT NULL
- `reset_day` INTEGER NOT NULL
- `last_reset_at` TEXT NULL
- `display` TEXT NULL
- `created_at` TEXT NOT NULL
- `updated_at` TEXT NOT NULL

Primary key:

- `(user_id, service_id, cycle_start_at, bucket_at)`

Rules:

- container panel sync writes at most one row per machine per hour bucket, and overwrites that bucket with the latest successful sample in the same hour;
- stored samples are scoped by `user_id` and deleted when the account disconnects or the machine disappears from an authoritative non-empty cache refresh;
- failed main-site discovery or empty parse must not be treated as machine disappearance for deletion purposes;
- `cycle_start_at` / `cycle_end_at` identify the billing cycle that the sample belongs to, so the API can return only the current cycle’s real history.
