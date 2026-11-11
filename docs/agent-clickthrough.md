# Agent click-through with reach

How a Claude Code session drives a reach sandbox for web click-throughs, with a
human signing in through noVNC when a site needs it.

Everything here was run on a WSL2 host with rootless docker (see "Verified").

## 1. Start a sandbox

```sh
make image                                   # docker build -t reach .
reach create --name reach --image reach      # add --persist-profile work to keep logins
reach list
```

`reach create` waits for `/health`, then prints the noVNC URL
(`http://localhost:6080`), the health API (`http://localhost:8400/health`) and
CDP (`http://localhost:8401/cdp`). Published ports bind to `127.0.0.1` only
(they are unauthenticated and give full control of the sandbox); reach them
from another machine with an SSH tunnel.

`reach` finds docker through `DOCKER_HOST`, then the current docker context,
then `$XDG_RUNTIME_DIR/docker.sock`, then `/var/run/docker.sock`. If none exists
the error lists every path tried.

Clean up with `reach destroy <name>`.

## 2. Connect Claude Code

There are two transports. Both expose the same tools (`screenshot`, `browse`,
`click`, `type`, `key`, `page_text`, `auth_handoff`, `exec`, `scrape_*`,
`browser_*`, ...).

### stdio bridge (recommended)

Claude Code spawns the bridge itself, so there is no server to keep running:

```sh
claude mcp add reach -- /absolute/path/to/reach connect reach
```

The last `reach` is the sandbox name. Use the absolute path of a `reach` built
from a tree that includes the docker endpoint fix; an old binary on `PATH` fails
with `Socket not found: /var/run/docker.sock` on rootless hosts. Add
`-s user` or `-s project` to pick the scope.

stdout carries only JSON-RPC; logs go to stderr.

### SSE/HTTP server on 4200

```sh
reach serve --sandbox reach          # 127.0.0.1:4200, endpoint POST /mcp
claude mcp add --transport http reach http://127.0.0.1:4200/mcp
```

`reach serve` binds to `127.0.0.1` by default. Keep it that way: the endpoint has
no authentication.

## 3. Typical click-through

1. `browse` the URL (or `page_text` for a JS-heavy page; it returns rendered text).
2. `screenshot`, then look at the PNG to decide the next action.
3. `click` / `type` / `key` with screen coordinates, `screenshot` again.
4. Repeat. `reach screenshot <name> -o out.png` does the same from a shell.

## 4. `auth_handoff`: a human signs in, the agent continues

Create the sandbox with a named persistent profile so the login outlives the
container:

```sh
reach create --name reach --image reach --persist-profile work
```

Flow:

1. The agent calls `auth_handoff` with `url`, `use_profile: "work"`, and a
   completion signal (`wait_for_url_contains` and/or `wait_for_selector`),
   e.g. `{"url": "https://app.example.com/login", "use_profile": "work",
   "wait_for_url_contains": "/dashboard", "timeout_seconds": 300}`.
2. The tool opens the URL in a headed Chrome on the sandbox display and **blocks**
   until the signal appears or the timeout passes. The result contains `vnc_url`.
3. The agent tells the human to open `vnc_url`
   (`http://localhost:6080/vnc.html?autoconnect=1&resize=remote`) and sign in,
   including any 2FA. The human is typing into the same Chrome the tool is polling.
4. When the signal appears the tool returns `status: "authenticated"` and closes
   that Chrome. On timeout it returns `status: "timeout"`; call it again.
5. The agent continues with `page_text` (or `auth_handoff` again) using the same
   `use_profile`. Cookies live in the profile directory
   (`~/.local/share/reach/profiles/work` on the host), so the session survives
   `reach destroy` and a new `reach create --persist-profile work`.

Important: `auth_handoff` and `page_text` launch their own Chrome on that
profile. The always-on Chrome that `browse`, `click`, `screenshot` and `browser_*`
use is a separate instance with a throwaway profile, so a login done through
`auth_handoff` is visible to `page_text` and further `auth_handoff` calls, not to
`browse`/`click`.

## Verified (2026-11-11, rootless docker, WSL2)

- `reach create`, `reach screenshot` (1280x720 PNG of example.com rendered in the
  sandbox Chrome), `reach destroy`.
- `page_text` on `https://example.com` through both `reach serve` (`POST /mcp`) and
  `reach connect` (stdio).
- `auth_handoff`: `status: "authenticated"` when the URL signal was already
  satisfied, `status: "timeout"` for a selector that never appears.
- `ss -ltnp`: ports 5900, 6080, 8400 and 8401 listen on `127.0.0.1` only.

Not verified: a real human sign-in through noVNC, `--persist-profile` across a
destroy/create (issue #8 tracks a uid mismatch on the profile mount), and the
`claude mcp add` commands above against a live Claude Code client (they are
documented, not run).
