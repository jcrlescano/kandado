-- Accounts, wrapped keys, devices and the Google sign-in handoff (phase 1).
-- Every `*_enc` / `*_wrapped` / `user_key_*` column is opaque bytes: the
-- server stores them and never reads them.

CREATE TABLE users (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    email        text NOT NULL,
    display_name text NOT NULL DEFAULT '',
    plan         text NOT NULL DEFAULT 'free',
    created_at   timestamptz NOT NULL DEFAULT now(),
    deleted_at   timestamptz
);

-- One live account per address. Deleted accounts keep their row, so the
-- address is free again for a new one.
CREATE UNIQUE INDEX users_email_live ON users (lower(email)) WHERE deleted_at IS NULL;

-- Which outside identity belongs to which user. Keyed by provider so a second
-- provider needs no change to `users`.
CREATE TABLE oauth_identities (
    provider   text NOT NULL,
    subject    text NOT NULL,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (provider, subject)
);
CREATE INDEX oauth_identities_user ON oauth_identities (user_id);

-- Phase 1 stores the X25519 pair only. The Ed25519 signing key in
-- docs/architecture/security.md joins when something first signs (phase 6).
CREATE TABLE user_keys (
    user_id              uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    version              smallint NOT NULL,
    kdf_salt             bytea NOT NULL,
    kdf_memory_kib       integer NOT NULL,
    kdf_iterations       integer NOT NULL,
    kdf_parallelism      integer NOT NULL,
    user_key_by_master   bytea NOT NULL,
    user_key_by_recovery bytea NOT NULL,
    public_x25519        bytea NOT NULL,
    private_keys_enc     bytea NOT NULL,
    updated_at           timestamptz NOT NULL DEFAULT now()
);

-- A signed-in session on one device (D-49). Refresh tokens are stored as
-- SHA-256 hashes. The previous hash is kept so that presenting an
-- already-rotated token is recognised as theft and revokes the device.
CREATE TABLE devices (
    id                          uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id                     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name                        text NOT NULL,
    platform                    text NOT NULL,
    refresh_token_hash          bytea NOT NULL,
    previous_refresh_token_hash bytea,
    created_at                  timestamptz NOT NULL DEFAULT now(),
    last_seen_at                timestamptz NOT NULL DEFAULT now(),
    -- When the person last signed in with Google; step-up checks read this.
    authenticated_at            timestamptz NOT NULL DEFAULT now(),
    revoked_at                  timestamptz
);
CREATE INDEX devices_user ON devices (user_id) WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX devices_refresh_hash ON devices (refresh_token_hash);

-- State of a Google flow between /start and /callback. Short-lived.
CREATE TABLE auth_flows (
    state_hash       bytea PRIMARY KEY,
    nonce            text NOT NULL,
    pkce_verifier    text NOT NULL,
    client           text NOT NULL,
    redirect         text NOT NULL,
    client_challenge text NOT NULL,
    purpose          text NOT NULL,
    -- Step-up only: the user and device being confirmed.
    user_id          uuid REFERENCES users (id) ON DELETE CASCADE,
    device_id        uuid,
    expires_at       timestamptz NOT NULL
);
CREATE INDEX auth_flows_expiry ON auth_flows (expires_at);

-- Single-use codes the client trades for tokens (60 seconds).
CREATE TABLE auth_handoffs (
    code_hash  bytea PRIMARY KEY,
    challenge  text NOT NULL,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    purpose    text NOT NULL,
    device_id  uuid,
    -- When the person actually signed in with Google; carried into the access
    -- token so sensitive routes can demand a recent one.
    auth_time  timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);
CREATE INDEX auth_handoffs_expiry ON auth_handoffs (expires_at);

CREATE TABLE audit_events (
    id        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    device_id uuid,
    kind      text NOT NULL,
    at        timestamptz NOT NULL DEFAULT now(),
    detail    jsonb
);
CREATE INDEX audit_events_user ON audit_events (user_id, at DESC);
