# WinSill

<p align="center">
  <img src="images/creatures.gif" alt="Eight pixel-art creatures, one per session state: working, needs permission, asking you, compacting, done, asleep, error and closed" width="100%">
</p>

One animated pixel-art creature per Claude Code session, on the edge of your Windows screen. Each creature shows what its session is doing, and the ones waiting for you are hard to miss.

WinSill is a Windows port of [Perch](https://github.com/elrincondeldev/Perch) by [elrincondeldev](https://github.com/elrincondeldev), which does this on macOS. **On a Mac, use Perch.**

> **Work in progress: there is nothing to install yet.** Both parts, the agent that tracks your sessions and the strip that draws the creatures, run on Windows from source (below). There is no installer: you start each by hand.

> **Not an official Anthropic project.** It is not affiliated with or endorsed by Anthropic. "Claude" is a trademark of Anthropic, PBC.

## Where it stands

| Part | On Windows |
|---|---|
| Agent: tracks every session, keeps pins and icons, opens a session in the Claude desktop app | Runs from source |
| Claude Code hooks, for instant and detailed state changes | `setup` installs them |
| The strip with the creatures (native, in Rust): states, label, menus, tray icon, collapse, left or right edge | Runs from source |
| Start at login, installer, the strip starting the agent | Not started |
| Phone view | Not started |

The macOS code (the Swift overlay in `Sources/`, `setup` with LaunchAgents) is kept as it is in Perch. It is not built or tested in this fork.

## Try the agent

You need Windows 10 or 11, [Bun](https://bun.sh) 1.3 or newer, the Claude Code CLI 2.1.285 or newer, and the Claude desktop app.

```bash
cd agent
bun install
bun src/perch.ts doctor    # checks everything the agent needs; changes nothing
bun src/perch.ts agent     # runs until Ctrl+C
```

The agent prints each session as its state changes. As it is, it knows which sessions are running and whether each is busy or idle. For the rest (what a session is doing, that it waits for your permission, that it failed), add the hooks:

```bash
bun src/perch.ts setup
```

That adds hooks to Claude Code's `settings.json`, after backing it up; `bun src/perch.ts uninstall` removes them. While the agent isn't running the hooks do nothing: Claude Code carries on.

The agent listens on 127.0.0.1 only. It keeps its tokens, its database and its log in `%LOCALAPPDATA%\winsill`.

## Try the strip

With the agent running, in another terminal. You need [Rust](https://rustup.rs) and the Visual Studio C++ Build Tools, which Rust's installer asks for.

```bash
cd strip
cargo run --release
```

The strip sits on the right edge of your main screen, centered above the taskbar. It shows the pinned sessions and the ones working or waiting for you; the `+` at the end pins any session from the Claude app. Click a creature to open its session, right-click it to pin it or change its look, click the arrow to collapse the strip. The tray icon has the menu (hide, left or right edge, quit). It remembers its choices in `%LOCALAPPDATA%\winsill\strip.json`.

It is one 0.4 MB program, about 13 MB of memory and next to no processor time. Its drawing, layout and protocol are tested apart from Windows (`cargo test`), and `cargo run --example preview -- out.png` draws a sample strip to a PNG.

## What changes from Perch

Everything that differs between the two systems is in [`agent/src/platform/`](agent/src/platform): where the agent keeps its data, where the Claude desktop app keeps its own, how a `claude://` link is opened and how hooks reach the agent.

- **Hooks** are `http` hooks, posted by Claude Code itself, where Perch pipes them through curl: on Windows a command hook runs in Git Bash or PowerShell. Claude Code skips http hooks on `SessionStart`, so that one event is not used.
- **The hook port** takes an event only with the token `setup` wrote into the hook, the right `Host` and a JSON content type, and only for a session id that looks like one. Loopback alone is not private: any web page can make a browser post to it.
- **Running sessions** come from the files Claude Code keeps per process (`sessions\<pid>.json` in its config folder), read when they change. Perch runs `claude agents --json` every 3 seconds; here that command is only the fallback.
- **The session in front** comes from the sessions' own files when the desktop app's log is silent, as it has been seen to be on Windows.
- **The desktop app's data** can be in `%APPDATA%\Claude` or in the app's MSIX package folder (`%LOCALAPPDATA%\Packages\Claude_…\LocalCache\Roaming\Claude`). The agent uses the one that has sessions, or the one written last if both do.
- **`CLAUDE_CONFIG_DIR`** is respected: Claude Code's `settings.json` and its live sessions are read from there when it is set.
- **Paths** get one spelling (`C:\Users\me`), since Claude's sources report the same folder as `c:/Users/me` or `C:\Users\me`.
- **`settings.json`** is replaced in one step, after a backup, so Claude Code never reads a half-written file.
- **The log** is the agent's own when no terminal shows it: `logs\agent.log`, kept under 3 MB.
- **`doctor`** says which of all this was found on your computer, and what works with less when something wasn't.

For how the agent works, its protocol and the creature templates, see [Perch's README](https://github.com/elrincondeldev/Perch#how-it-works): that part is the same.

## Development

```bash
(cd agent && bun install)
bun test agent npm               # agent, CLI and npm launcher
(cd agent && bun run typecheck)
(cd strip && cargo test)
```

## Privacy

- Everything stays on your computer: the agent listens on 127.0.0.1 only, unless you start it with `--lan`. No telemetry.
- Its WebSocket needs the pairing token in `%LOCALAPPDATA%\winsill\config.json`, and accepts only `open`, `pin`, `icon` and `library`, with validated arguments.
- The hooks carry a second token, which `setup` writes into Claude Code's `settings.json`. If you share that file, anyone who can run programs on your computer could send the agent made-up events; they could not read anything with it.
- A session's label can show tool details such as a shell command, and the agent's log keeps those lines.

## Credits and license

[MIT](LICENSE). WinSill is based on [Perch](https://github.com/elrincondeldev/Perch), © 2026 Jose Ramon Montes Hermida, also MIT: the agent, the protocol and the creatures are his work. Two of the fixes here come from [Staberman's pull request](https://github.com/elrincondeldev/Perch/pull/9) to Perch.
