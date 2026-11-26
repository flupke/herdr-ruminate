# Other outside systems

## cloudflared and Cloudflare quick tunnels

- A quick tunnel's link can fail for a few seconds after `cloudflared` prints it, while Cloudflare registers the tunnel; a browser then shows a Cloudflare error page (version not recorded).
- `cloudflared` forwards the public `Host` and an `https://` `Origin` to the local address (version not recorded).
- Traffic is encrypted from the browser to Cloudflare and from Cloudflare to `cloudflared`, but Cloudflare sees the content (version not recorded).
- A named tunnel needs a Cloudflare account and DNS; a quick tunnel needs neither (version not recorded).

## Browsers

- Browsers keep cookies by host name, not by port: a cookie set by one port of `127.0.0.1` goes to every server on that address, and any of them can set cookies under the same names (version not recorded).
- A self-signed HTTPS certificate makes every phone warn, and needs an exception for each new certificate (version not recorded).

## Terminals

- While an application captures the mouse, the terminal's own selection does not reach it; Shift and a drag still makes the terminal's own selection (version not recorded).
- A terminal may or may not report a modifier such as Alt on the moves of a drag: only the press's modifier is reliable (version not recorded).

## Ratatui

- Ratatui's word wrapper returns rows, not where it broke a line, and drops the space it broke at (version not recorded).

## Linux

- A `flock` is held per open file: a thread that opens and locks the same file twice waits forever (version not recorded).
- A TCP port on localhost is open to every local user, who can forge a request; a directory under `$XDG_RUNTIME_DIR` is the user's alone (version not recorded).
