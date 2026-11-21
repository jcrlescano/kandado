# Deploying kandado-server to the droplet

Decision 16: the server runs on the DigitalOcean droplet beside Hatian, on
`127.0.0.1:8081` behind the droplet's nginx, with its own DuckDNS name and a
Let's Encrypt certificate, and a database in Hatian's PostgreSQL. GitHub
Actions deploys it after every change to `main` that passes the checks
(`.github/workflows/deploy-server.yml`).

```
GitHub Actions ── checks ── build (Ubuntu 22.04) ── scp + ssh ──► droplet
                                                                    │
  https://KANDADO_HOST ─► nginx (certbot) ─► 127.0.0.1:8081 kandado-server (systemd, user kandado)
                                                         └─► Hatian's PostgreSQL, database `kandado`
```

Hatian is not touched: its nginx site, port and database stay as they are.

## One-time setup on the droplet

Replace `KANDADO_HOST` with the DuckDNS name and `DEPLOYER` with the SSH user
the pipeline logs in as. Run these as a user with sudo.

### 1. A DuckDNS name

On duckdns.org, add a name (for example `kandado-juan`) and point it at the
droplet's public IP, the same IP Hatian's name uses. `KANDADO_HOST` is then
`kandado-juan.duckdns.org`.

### 2. PostgreSQL 18 or later

Kandado's first migration refuses anything older (every ID uses the built-in `uuidv7()`).

```sh
sudo -u postgres psql -c 'SHOW server_version;'
```

If it is below 18, stop here: upgrading Hatian's PostgreSQL is a decision of its
own. The alternative is a separate PostgreSQL 18 for Kandado (a container bound
to `127.0.0.1`).

Then a role and a database that only Kandado uses:

```sh
openssl rand -base64 32          # the role's password: keep it for step 5
sudo -u postgres psql <<'SQL'
CREATE ROLE kandado LOGIN PASSWORD 'PASTE_THE_PASSWORD';
CREATE DATABASE kandado OWNER kandado;
REVOKE ALL ON DATABASE kandado FROM PUBLIC;
SQL
```

### 3. Users and folders

```sh
# The service's own user: no login, no shell.
sudo useradd --system --home-dir /opt/kandado --no-create-home --shell /usr/sbin/nologin kandado
# The pipeline's user owns the releases; the service only reads them.
sudo install -d -o DEPLOYER -g kandado -m 755 /opt/kandado
# Google sign-in settings: the pipeline writes them, the service reads them.
# setgid, so files made here belong to group kandado.
sudo install -d -o DEPLOYER -g kandado -m 2750 /opt/kandado/config
```

Do not add `DEPLOYER` to the `kandado` group: it would then read the database
password and token secret in `/etc/kandado/kandado.env`.

`DEPLOYER` can be a new user (`sudo adduser --disabled-password kandado-deploy`)
or the one Hatian's deploys use. A separate one is safer.

### 4. Let the pipeline restart the service, and nothing else

```sh
echo 'DEPLOYER ALL=(root) NOPASSWD: /usr/bin/systemctl restart kandado-server' \
  | sudo tee /etc/sudoers.d/kandado-deploy
sudo chmod 440 /etc/sudoers.d/kandado-deploy
sudo visudo -c
```

### 5. Settings

```sh
sudo install -d -m 750 -o root -g kandado /etc/kandado
sudo install -m 640 -o root -g kandado kandado.env.example /etc/kandado/kandado.env
sudoedit /etc/kandado/kandado.env
```

Fill in `KANDADO_PUBLIC_URL`, the database password in `DATABASE_URL`, and a
new `KANDADO_TOKEN_SECRET` (`openssl rand -base64 48`). Google sign-in comes
from GitHub secrets (step 8); without it the server runs but nobody can sign in.

### 6. The service

```sh
sudo cp kandado-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable kandado-server     # starts once the first release is in place
```

### 7. nginx and the certificate

```sh
sed 's/KANDADO_HOST/kandado-juan.duckdns.org/g' nginx-kandado.conf \
  | sudo tee /etc/nginx/sites-available/kandado
sudo ln -s /etc/nginx/sites-available/kandado /etc/nginx/sites-enabled/kandado
sudo nginx -t && sudo systemctl reload nginx
sudo certbot --nginx -d kandado-juan.duckdns.org
```

Port 8081 listens on `127.0.0.1` only, so no firewall change is needed: only
80 and 443, which Hatian already uses, face the internet.

### 8. Google sign-in

In Google Cloud Console › APIs & Services › Credentials, create an OAuth client
of type "Web application" with the authorised redirect URI
`https://KANDADO_HOST/v1/auth/google/callback`. Add its ID and secret as the
`GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` secrets of the `production`
environment (below). Each deploy sends them over the SSH connection's standard
input to `/opt/kandado/config/google.env` (mode 640, group `kandado`), never
on a command line; the service reads that file after `kandado.env`. To change
them, update the secrets and run the deploy again.

The same file gets `KANDADO_EXTENSION_REDIRECT_HOSTS` on each deploy: the
browser extensions' sign-in redirect hosts, worked out from
`src/extensions/manifests/release.json` (`node scripts/extension-id.mjs --host`)
and shipped in the release as `redirect-hosts`. Nothing to set by hand; the
pipeline's list wins over the line in `kandado.env`.

## The pipeline's access

Make a key for the pipeline only, on your own computer:

```sh
ssh-keygen -t ed25519 -f ~/.ssh/kandado_deploy -C "kandado github deploy" -N ""
```

Add the public half (`~/.ssh/kandado_deploy.pub`) to
`/home/DEPLOYER/.ssh/authorized_keys` on the droplet. Then record the
droplet's host key, and check its fingerprint against the droplet's own
(`ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` on the droplet):

```sh
ssh-keyscan -t ed25519 DROPLET_IP > droplet_known_hosts
ssh-keygen -lf droplet_known_hosts
```

In GitHub › the repository › Settings › Environments, create `production`
(optionally with yourself as a required reviewer, so each deploy waits for a
click) and add:

| Kind | Name | Value |
| --- | --- | --- |
| Secret | `DEPLOY_HOST` | The droplet's IP address |
| Secret | `DEPLOY_USER` | `DEPLOYER` |
| Secret | `DEPLOY_SSH_KEY` | The whole of `~/.ssh/kandado_deploy` (the private half) |
| Secret | `DEPLOY_KNOWN_HOSTS` | The line in `droplet_known_hosts` |
| Secret | `GOOGLE_CLIENT_ID` | The Google OAuth client's ID (optional; both or neither) |
| Secret | `GOOGLE_CLIENT_SECRET` | Its secret |
| Variable | `KANDADO_PUBLIC_URL` | `https://KANDADO_HOST` |

## Deploying

Every push to `main` that touches the server, the shared crates or the link
viewer runs the full checks, builds on Ubuntu 22.04, uploads the release and
runs `kandado-deploy.sh` on the droplet. Actions › Deploy server › Run workflow
deploys by hand.

`kandado-deploy.sh` unpacks the release to `/opt/kandado/releases/<commit>`,
points `/opt/kandado/current` at it and restarts the service. If `/readyz` does
not answer within a minute it switches back to the previous release, restarts,
and fails the job. It keeps the five newest releases. Migrations run when the
server starts; they only ever add, so the previous release still runs on the
newer schema.

## Looking at it

```sh
systemctl status kandado-server
journalctl -u kandado-server -f          # JSON logs; never tokens or bodies
curl -s http://127.0.0.1:8081/readyz
ls -l /opt/kandado/current               # which release is live
```

Rolling back by hand: `ln -sfn /opt/kandado/releases/<commit> /opt/kandado/current.next && mv -Tf /opt/kandado/current.next /opt/kandado/current && sudo systemctl restart kandado-server`.

## Backups

The database lives in Hatian's PostgreSQL, so Hatian's backups cover it if they
dump every database. A dump of Kandado alone:
`sudo -u postgres pg_dump -Fc kandado > kandado-$(date +%F).dump`. Everything in
it is ciphertext, but keep it private all the same: it holds account emails.
