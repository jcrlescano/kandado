-- Vaults, items and their versions (phase 3). Every `*_enc`, `*_wrapped` and
-- `*_sealed` column is opaque bytes: the server stores them and never reads
-- them. What it does see is what it needs to order and authorise changes.

-- One counter for every row that syncs. A row's `revision` is the value it was
-- last written with, so `revision > N` means "changed after N".
CREATE SEQUENCE sync_revision;

CREATE TABLE vaults (
    id         uuid PRIMARY KEY,
    owner_id   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    meta_enc   bytea NOT NULL,
    revision   bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);
CREATE INDEX vaults_owner ON vaults (owner_id);

-- Owner only until sharing (phase 6), but queried through here already, so
-- sharing changes who is in this table and nothing else.
CREATE TABLE vault_members (
    vault_id         uuid NOT NULL REFERENCES vaults (id) ON DELETE CASCADE,
    user_id          uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role             text NOT NULL CHECK (role IN ('viewer', 'editor', 'manager', 'owner')),
    vault_key_sealed bytea NOT NULL,
    PRIMARY KEY (vault_id, user_id)
);
CREATE INDEX vault_members_user ON vault_members (user_id);

CREATE TABLE items (
    id               uuid PRIMARY KEY,
    vault_id         uuid NOT NULL REFERENCES vaults (id) ON DELETE CASCADE,
    -- Plaintext on purpose (decision A6): lets the server count and filter.
    kind             text NOT NULL
        CHECK (kind IN ('login', 'alias', 'card', 'note', 'identity', 'password', 'passkey')),
    item_key_wrapped bytea NOT NULL,
    content_enc      bytea NOT NULL,
    -- Names the saved version of the content, the same on every device.
    version_id       uuid NOT NULL,
    revision         bigint NOT NULL,
    -- From the device that made the change, in Unix seconds.
    created_at       bigint NOT NULL,
    updated_at       bigint NOT NULL,
    trashed_at       bigint,
    -- When the server first saw it in the trash. The purge reads this, not
    -- a device clock.
    trashed_on       timestamptz,
    -- A tombstone: content and history are gone, the row stays so other
    -- devices learn of the deletion.
    deleted_at       timestamptz
);
CREATE INDEX items_vault_revision ON items (vault_id, revision);
CREATE INDEX items_trashed ON items (trashed_on)
    WHERE trashed_on IS NOT NULL AND deleted_at IS NULL;

-- Every saved version of an item, including the current one (D-16).
CREATE TABLE item_revisions (
    item_id          uuid NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    version_id       uuid NOT NULL,
    content_enc      bytea NOT NULL,
    item_key_wrapped bytea NOT NULL,
    author_id        uuid REFERENCES users (id) ON DELETE SET NULL,
    saved_at         bigint NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (item_id, version_id)
);
