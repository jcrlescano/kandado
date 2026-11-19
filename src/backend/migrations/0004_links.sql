-- Secure links (phase 6, D-32 and D-33): one item shared with anyone who has
-- the link. `payload_enc` is the item's snapshot encrypted with a key that
-- exists only in the link's fragment; the server cannot open it. It is set to
-- NULL as soon as the link stops working, so nothing is kept that could be
-- handed out later.

CREATE TABLE secure_links (
    -- Chosen by the device, which encrypts the payload to it first.
    id          uuid PRIMARY KEY,
    owner_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    item_id     uuid NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    payload_enc bytea,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL,
    -- NULL: no limit.
    max_views   integer CHECK (max_views IS NULL OR max_views > 0),
    views       integer NOT NULL DEFAULT 0 CHECK (views >= 0),
    revoked_at  timestamptz
);
CREATE INDEX secure_links_owner ON secure_links (owner_id, created_at DESC);
CREATE INDEX secure_links_item ON secure_links (item_id);
-- The expiry job looks only at links that still hold a payload.
CREATE INDEX secure_links_live ON secure_links (expires_at) WHERE payload_enc IS NOT NULL;
