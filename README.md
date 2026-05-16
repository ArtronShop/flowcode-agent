# FlowCode Agent

A lightweight Windows background agent that bridges [FlowCode](https://github.com/ArtronShop/flowcode) (web app) with the Arduino CLI installed on your machine. It exposes a WebSocket API so FlowCode can compile sketches, upload to boards, manage libraries, and communicate over serial — all from the browser.

---

## Usage

### Prerequisites

- **Arduino IDE 2** must be installed. FlowCode Agent uses the `arduino-cli` bundled with it.

### Installation

1. Download the latest `flowcode-agent-V2.x.x.exe` from [Releases](../../releases).
2. Run the file — no installation required. The agent starts immediately and appears in the system tray.
3. A config file is created automatically at:
   ```
   %LocalAppData%\FlowcodeAgent\configs.json
   ```

> Only one instance can run at a time. Opening a second copy exits immediately.

### Closing the Agent

Right-click the FlowCode Agent icon in the system tray → **Exit**.

### Configuration

Right-click the system tray icon → **Settings...**

This opens `configs.json` in Notepad. Save and close the file — settings reload automatically.

| Key | Description | Default |
|-----|-------------|---------|
| `arduino_cli_path` | Path to `arduino-cli.exe` | Bundled path inside Arduino IDE 2 |
| `arduino_data_path` | Arduino data directory | `%LocalAppData%\Arduino15` |
| `arduino_downloads_path` | Arduino downloads/staging directory | `%LocalAppData%\Arduino15\staging` |
| `arduino_sketch_path` | Sketchbook directory | `Documents\Arduino` |
| `auto_start` | Start automatically on Windows boot | `true` |
| `arduino_preferences_path` | Path to Arduino IDE `preferences.txt` | Auto-detected |
| `arduino_sketch_path_from_preferences` | Read sketchbook path from `preferences.txt` | `true` |
| `additional_urls_from_preferences` | Read additional board URLs from `preferences.txt` | `true` |

---

## How It Works

FlowCode Agent runs a WebSocket server on port `8080` (configurable via `PORT` env var).

- WebSocket: `ws://localhost:8080/`

The browser connects via WebSocket and sends JSON messages:

```json
{ "id": "req-1", "action": "compile", "params": { "sketch": "MySketch", "fqbn": "arduino:avr:uno" } }
```

The agent executes the corresponding `arduino-cli` command and sends back results:

```json
{ "id": "req-1", "type": "result", "payload": { "ok": true } }
```

Long-running actions (`compile`, `upload`, `core.install`, `lib.install`) stream stdout/stderr in real-time:

```json
{ "id": "req-1", "type": "stream", "payload": { "stream": "stdout", "data": "..." } }
```

### Supported Actions

| Action | Params | Description |
|--------|--------|-------------|
| `board.list` | — | List connected boards |
| `board.listall` | `fqbn?` | List all supported boards |
| `core.install` | `id`, `version`, `package_index?` | Install an Arduino core (streaming) |
| `lib.install` | `depends: string[]` | Install libraries (streaming) |
| `sketch.list` | — | List all sketches |
| `sketch.create` | `name`, `code?` | Create a new sketch |
| `sketch.read` | `name` | Read sketch source code |
| `sketch.write` | `name`, `code` | Write sketch source code |
| `sketch.delete` | `name` | Delete a sketch |
| `compile` | `sketch`, `fqbn`, `boardOption?` | Compile a sketch (streaming) |
| `upload` | `sketch`, `fqbn`, `port`, `boardOption?` | Upload a sketch to a board (streaming) |
| `port.list` | — | List available serial ports |
| `port.connect` | `port`, `baudRate?` | Open a serial port and stream incoming data |
| `port.disconnect` | `port` | Close a serial port |
| `port.write` | `port`, `data` | Send data to a serial port |
| `version` | — | Get arduino-cli version |
| `config.init` | `additional_urls?` | Initialize arduino-cli config and directories |

### Response Types

| `type` | When |
|--------|------|
| `result` | Action completed successfully |
| `stream` | Real-time stdout/stderr from a running process |
| `port.data` | Data received from an open serial port |
| `port.close` | Serial port was disconnected |
| `error` | Action failed |

---

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 1.75+)
- Arduino IDE 2 (for `arduino-cli`)

### Setup

```bash
git clone https://github.com/ArtronShop/flowcode-agent.git
cd flowcode-agent
```

### Run in dev mode

```bash
cargo run
```

The server starts on `ws://localhost:8080/`. A console window is shown in dev mode so you can see log output.

### Project structure

```
flowcode-agent/
├── Cargo.toml          # Rust project & dependencies
├── build.rs            # Embeds logo.ico into the exe at compile time
├── build.ps1           # Release build script → build/flowcode-agent-Vx.x.x.exe
├── asset/
│   ├── logo.ico        # App icon (embedded into exe)
│   └── logo.png        # Tray icon (embedded into binary)
└── src/
    ├── main.rs         # WebSocket server + system tray + single-instance guard
    ├── configs.rs      # Config file management
    └── arduino.rs      # arduino-cli wrapper + sketch helpers
```

---

## Building

Build a versioned release exe using the provided script:

```powershell
.\build.ps1
```

Output: `build\flowcode-agent-V2.x.x.exe`

The script reads the version from `Cargo.toml` automatically. The binary has the app icon embedded and shows no console window when launched normally. Running it from a terminal (cmd / PowerShell) still prints log output to that terminal.

To build manually without the script:

```bash
cargo build --release
# output: target/release/flowcode-agent.exe
```
