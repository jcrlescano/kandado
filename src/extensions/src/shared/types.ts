// Wire types of the Kandado API, mirroring `kandado-protocol`. Binary fields are
// base64 strings on the wire; the rest of the extension holds them as bytes.

export type ItemKind = "login" | "alias" | "card" | "note" | "identity" | "password" | "passkey";

export interface WireVault {
  id: string;
  key_sealed: string;
  meta_enc: string;
  revision: number;
}

export interface WireItem {
  id: string;
  vault_id: string;
  kind: ItemKind;
  key_wrapped: string;
  content_enc: string;
  version_id: string;
  created_at: number;
  updated_at: number;
  trashed_at: number | null;
  deleted?: boolean;
  revision: number;
}

export interface PullResponse {
  vaults: WireVault[];
  items: WireItem[];
  cursor: number;
  more: boolean;
}

export interface PushRequest {
  vaults: WireVault[];
  items: WireItem[];
}

export type ChangeResult<T> =
  | { status: "applied"; revision: number }
  | { status: "conflict"; current: T }
  | { status: "rejected" };

export interface PushResponse {
  vaults: ChangeResult<WireVault>[];
  items: ChangeResult<WireItem>[];
  cursor: number;
}

export interface TokenResponse {
  access_token: string;
  expires_in: number;
  refresh_token?: string;
  user_id: string;
  device_id: string;
}

export interface Me {
  id: string;
  email: string;
  display_name: string;
  plan: string;
  has_keys: boolean;
  device_id: string;
}

/** `GET /v1/account/keys`, kept as the JSON text the WebAssembly core reads. */
export type AccountKeysJson = string;

/** RFC 9457 problem document the server answers errors with. */
export interface Problem {
  type?: string;
  title?: string;
  status: number;
  code?: string;
  detail?: string;
}
