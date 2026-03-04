# Claude Code Project Context

## Project Overview
claude-tui is a terminal multiplexer for managing multiple Claude Code sessions. It provides a tmux-like interface with a sidebar showing session status, context usage, and attention indicators.

## Tech Stack
- **Language**: Rust (2021 edition)
- **TUI Framework**: ratatui + crossterm
- **Terminal Emulation**: vt100 crate for parsing escape sequences
- **PTY Management**: portable-pty for spawning and managing pseudo-terminals
- **HTTP**: reqwest with rustls-tls (no OpenSSL dependency)

## Architecture

### Key Modules
- `src/main.rs` - Entry point, event loop, terminal setup
- `src/app.rs` - Application state, session management
- `src/session.rs` - Individual Claude Code session wrapper with PTY
- `src/input.rs` - Keyboard/mouse input handling with tmux-like prefix (Ctrl+B)
- `src/ui/` - Rendering (sidebar, content area, tiled view)
- `src/config.rs` - TOML configuration loading
- `src/usage.rs` - API usage tracking from Claude's usage.json
- `src/persistence.rs` - Session state save/restore

### Key Patterns
- Sessions are spawned as child processes via PTY
- vt100 parser maintains virtual screen state with scrollback
- Status detection via regex patterns on terminal output (CTX:%, spinner chars)
- Attention system notifies when background sessions complete tasks

## Build Commands
```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo fmt                # Format code
cargo clippy             # Lint check
cargo test               # Run tests
```

## Configuration
Config file: `~/.config/claude-tui/config.toml` or `./config.toml`

```toml
claude_cmd = "claude"           # Path to claude binary
claude_config_dir = "~/.claude" # Optional: Claude config directory
```

## Keybindings
- `Ctrl+B` - Prefix key (like tmux)
- `Ctrl+B c` - New session in current directory
- `Ctrl+B C` - New session with directory picker
- `Ctrl+B n/p` - Next/previous session
- `Ctrl+B 1-9` - Jump to session
- `Ctrl+B t` - Toggle tiled view
- `Ctrl+B x` - Close session
- `Ctrl+Q` - Quit

## Code Style
- Use `cargo fmt` before committing
- All clippy warnings treated as errors in CI
- Prefer `rustls` over OpenSSL for cross-platform builds
- Use `dirs` crate for platform-independent paths (no hardcoded paths)
