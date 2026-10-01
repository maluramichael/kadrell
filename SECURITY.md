# Security Policy

## Supported versions

Only the latest release of Kadrell is supported with security fixes. Please make
sure you are on the newest build from
[kadrell.malura.de/download/Kadrell.dmg](https://kadrell.malura.de/download/Kadrell.dmg)
before reporting an issue.

## Threat model

The trust boundary is the macOS user. Kadrell does not defend against other processes running as the same user, and it is not sandboxed: the sandbox would block `claude` from `~/.claude`, your project folders and the keychain. What it does limit is what text shown in a terminal tile can make Kadrell do.

- **Control socket.** `kadrell.sock` lives in the profile folder with mode 0600, and the app checks the peer uid of every connection (`getpeereid`) against its own. Other users are rejected.
- **Cross-session control.** The setting "Sessions may control other sessions" (German UI: "Sessions dürfen andere Sessions steuern") is off by default. While it is off, a `kadrell` call coming from inside a tile can only address sessions of its own group: `send`, `capture`, `stop`, `kill` and the like. `kadrell new` from a tile starts only in the caller's own group and below that group's folder. `ls`, `select` and `layout` stay open. The app identifies the calling tile from the process behind the socket connection, not from the `KADRELL_SESSION_KEY` variable. Calls from outside Kadrell are never restricted. This is damage limitation against prompt injection, not a hard boundary: everything runs as the same user, so a process in a tile can still reach the socket of the profile or act on your files directly.
- **Hooks in every Claude process.** Each `claude` process Kadrell starts gets five status hooks through `--settings`. They only report status to the tile's own socket and exit at once without `KADRELL_SOCKET`. Nothing is written to `~/.claude/settings.json`. User hook scripts in `~/.config/kadrell/hooks/` run with your rights, but only if they and the folder belong to you and are not writable by group or others.
- **Keychain.** Kadrell reads the item `Claude Code-credentials` (Claude Code's own login) through `/usr/bin/security` and uses its OAuth token only against `api.anthropic.com` for the usage display and the account e-mail. With multiple accounts it keeps one item per account under the service `de.malura.kadrell.account` and writes `Claude Code-credentials` when you switch. These items are readable by processes of the same user, as is the original item, and are not protected from them any further.
- **Clipboard and links.** A program in a tile cannot read the clipboard through OSC 52 (reading is refused, writing still works). Terminal links to apps, scripts or executable files are shown in the Finder instead of being opened, so a program cannot choose link text that starts something.
- **No telemetry.** Kadrell has no server of its own. The only network traffic is the optional update check to `kadrell.malura.de` and the requests to `api.anthropic.com` listed in the [README](README.md#what-kadrell-touches).

## Reporting a vulnerability

Please report security vulnerabilities responsibly and privately.

- Email **michael@malura.de** with a description of the issue and, if possible,
  steps to reproduce it.
- Do **not** open a public GitHub issue for a security vulnerability, and do not
  disclose it publicly until it has been addressed.

You will get an acknowledgement of your report, and I will keep you informed as
the issue is investigated and fixed. Thank you for helping keep Kadrell and its
users safe.
