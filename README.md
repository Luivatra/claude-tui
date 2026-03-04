# claude-tui

A terminal multiplexer for managing multiple [Claude Code](https://claude.com/claude-code) sessions simultaneously.

![License](https://img.shields.io/badge/license-MIT-blue.svg)

## Features

- **Multiple Sessions**: Run several Claude Code instances side-by-side
- **Tmux-like Controls**: Familiar prefix-key navigation (Ctrl+B)
- **Session Status**: Visual indicators for thinking, idle, and completed states
- **Context Tracking**: See context window usage percentage per session
- **Attention System**: Get notified when background sessions complete tasks
- **Tiled View**: Split-screen view for monitoring multiple sessions
- **Session Persistence**: Save and restore sessions across restarts
- **Usage Monitoring**: Track 5-hour and weekly API usage limits

## Installation

### From Releases

Download the latest binary for your platform from the [releases page](https://github.com/luivatra/claude-tui/releases).

**Linux (x86_64)**:
```bash
curl -LO https://github.com/luivatra/claude-tui/releases/latest/download/claude-tui-x86_64-unknown-linux-gnu.tar.gz
tar xzf claude-tui-x86_64-unknown-linux-gnu.tar.gz
sudo mv claude-tui /usr/local/bin/
```

**macOS (Apple Silicon)**:
```bash
curl -LO https://github.com/luivatra/claude-tui/releases/latest/download/claude-tui-aarch64-apple-darwin.tar.gz
tar xzf claude-tui-aarch64-apple-darwin.tar.gz
sudo mv claude-tui /usr/local/bin/
```

### From Source

Requires [Rust](https://rustup.rs/) 1.70+:

```bash
git clone https://github.com/luivatra/claude-tui.git
cd claude-tui
cargo build --release
cp target/release/claude-tui ~/.local/bin/
```

## Usage

```bash
# Start with a new session in current directory
claude-tui

# Or specify a working directory
claude-tui --directory /path/to/project
```

## Keybindings

All commands use `Ctrl+B` as the prefix key (like tmux):

| Key | Action |
|-----|--------|
| `Ctrl+B c` | Create new session (current directory) |
| `Ctrl+B C` | Create new session (with directory picker) |
| `Ctrl+B n` | Next session |
| `Ctrl+B p` | Previous session |
| `Ctrl+B 1-9` | Jump to session by number |
| `Ctrl+B t` | Toggle tiled/fullscreen view |
| `Ctrl+B x` | Close current session |
| `Ctrl+Q` | Quit |

Mouse scrolling works in both fullscreen and tiled modes.

## Configuration

Create `~/.config/claude-tui/config.toml`:

```toml
# Path to claude binary (default: "claude")
claude_cmd = "claude"

# Optional: Custom Claude config directory
# claude_config_dir = "~/.claude"
```

## Session Status Icons

| Icon | Meaning |
|------|---------|
| `>` | Idle (ready for input) |
| `*` | Thinking (processing) |
| `~` | Running (executing tools) |
| `...` | Starting |
| `x` | Exited |

Sessions show a blinking `!` indicator when they complete a task while in the background.

## Requirements

- Claude Code CLI installed and configured
- Terminal with 256-color support recommended

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions welcome! Please run `cargo fmt` and `cargo clippy` before submitting PRs.
