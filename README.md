# WinSill

<p align="center">
  <img src="images/creatures.gif" alt="Eight pixel-art creatures, one per session state: working, needs permission, asking you, compacting, done, asleep, error and closed" width="100%">
</p>

One animated pixel-art creature per Claude Code session, on the edge of your Windows screen. Each creature shows what its session is doing, and the ones waiting for you are hard to miss.

WinSill is a Windows port of [Perch](https://github.com/elrincondeldev/Perch) by [elrincondeldev](https://github.com/elrincondeldev), which does this on macOS. **On a Mac, use Perch.**

> **Work in progress: there is nothing to install yet.** The agent that tracks your sessions runs on Windows; the strip that draws the creatures is not written.

> **Not an official Anthropic project.** It is not affiliated with or endorsed by Anthropic. "Claude" is a trademark of Anthropic, PBC.

## Where it stands

| Part | On Windows |
|---|---|
| Agent: tracks every session, keeps pins and icons, opens a session in the Claude desktop app | Runs from source |
| Claude Code hooks, for instant state changes | Not yet: state comes from `claude agents --json`, every 3 seconds |
| The strip with the creatures (native, in Rust) | Not started |
| `setup`, start at login, installer | Not started |
| Phone view | Not started |

The macOS code (the Swift overlay in `Sources/`, `setup` with LaunchAgents) is kept as it is in Perch. It is not built or tested in this fork.

## Try the agent

You need Windows 10 or 11, [Bun](https://bun.sh) 1.3 or newer, the Claude Code CLI 2.1.285 or newer on your `PATH`, and the Claude desktop app.

```bash
cd agent
bun install
bun src/perch.ts agent
```

It prints each session as its state changes, until you stop it with Ctrl+C. It listens on 127.0.0.1 only and does not touch Claude Code's settings. It keeps its pairing token and its database in `%LOCALAPPDATA%\winsill`; delete that folder to remove every trace.

`bun src/perch.ts status` shows whether the agent is answering.

## What changes from Perch

Everything that differs between the two systems is in [`agent/src/platform/`](agent/src/platform): where the agent keeps its data, where the Claude desktop app keeps its own, and how a `claude://` link is opened.

- **The desktop app's data** can be in `%APPDATA%\Claude` or in the app's MSIX package folder (`%LOCALAPPDATA%\Packages\Claude_…\LocalCache\Roaming\Claude`). The agent uses the one that has sessions, or the one written last if both do.
- **`CLAUDE_CONFIG_DIR`** is respected: Claude Code's `settings.json` and its live sessions are read from there when it is set.
- **Paths** get one spelling (`C:\Users\me`), since Claude's sources report the same folder as `c:/Users/me` or `C:\Users\me`.
- **`settings.json`** is replaced in one step, after a backup, so Claude Code never reads a half-written file.

For how the agent works, its protocol and the creature templates, see [Perch's README](https://github.com/elrincondeldev/Perch#how-it-works): that part is the same.

## Development

```bash
(cd agent && bun install)
bun test agent npm               # agent, CLI and npm launcher
(cd agent && bun run typecheck)
```

## Privacy

- Everything stays on your computer: the agent listens on 127.0.0.1 only, unless you start it with `--lan`. No telemetry.
- Its WebSocket needs the pairing token in `%LOCALAPPDATA%\winsill\config.json`, and accepts only `open`, `pin`, `icon` and `library`, with validated arguments.

## Credits and license

[MIT](LICENSE). WinSill is based on [Perch](https://github.com/elrincondeldev/Perch), © 2026 Jose Ramon Montes Hermida, also MIT: the agent, the protocol and the creatures are his work. Two of the fixes here come from [Staberman's pull request](https://github.com/elrincondeldev/Perch/pull/9) to Perch.
