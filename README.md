# FlowCode Agent

A lightweight Windows background agent that bridges [FlowCode](https://github.com/ArtronShop/flowcode) (web app) with the Arduino CLI installed on your machine. It exposes a WebSocket API so FlowCode can compile sketches, upload to boards, manage libraries, and communicate over serial — all from the browser.

---

## Usage

### Prerequisites

- **Arduino IDE 2** must be installed. FlowCode Agent uses the `arduino-cli` bundled with it.

### Installation

1. Download the latest `flowcode-agent-vX.X.X.exe` from [Releases](../../releases).
2. Run the file — no installation required. The agent starts immediately and appears in the system tray.
3. A config file is created automatically at:
   ```
   %LocalAppData%\FlowcodeAgent\configs.json
   ```

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

FlowCode Agent runs an HTTP + WebSocket server on `ws://localhost:8080`.

- The browser connects via WebSocket and sends JSON messages in the format:
  ```json
  { "id": "req-1", "action": "compile", "params": { "sketch": "MySketch", "fqbn": "arduino:avr:uno" } }
  ```
- The agent executes the corresponding `arduino-cli` command and sends back results:
  ```json
  { "id": "req-1", "type": "result", "payload": { "ok": true } }
  ```
- Long-running actions (`compile`, `upload`, `core.install`, `lib.install`) stream stdout/stderr in real-time:
  ```json
  { "id": "req-1", "type": "stream", "payload": { "stream": "stdout", "data": "..." } }
  ```

### Supported Actions

| Action | Description |
|--------|-------------|
| `board.list` | List connected boards |
| `board.listall` | List all supported boards |
| `core.install` | Install an Arduino core (streaming) |
| `lib.install` | Install libraries (streaming) |
| `sketch.list` | List all sketches |
| `sketch.create` | Create a new sketch |
| `sketch.read` | Read sketch source code |
| `sketch.write` | Write sketch source code |
| `sketch.delete` | Delete a sketch |
| `compile` | Compile a sketch (streaming) |
| `upload` | Upload a sketch to a board (streaming) |
| `port.list` | List available serial ports |
| `port.connect` | Open a serial port and stream incoming data |
| `port.disconnect` | Close a serial port |
| `port.write` | Send data to a serial port |
| `version` | Get arduino-cli version |
| `config.init` | Initialize arduino-cli config and directories |

---

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- Arduino IDE 2 (for `arduino-cli`)

### Setup

```bash
git clone https://github.com/ArtronShop/flowcode-agent.git
cd flowcode-agent
npm install
```

### Run in dev mode

```bash
npm run dev
```

The server starts on `http://localhost:8080`. Source files are watched — the process restarts automatically on save.

---

## Building

Build a standalone Windows executable (`.exe`):

```bash
npm run dist
```

This will:
1. Compile TypeScript → `dist/`
2. Download `rcedit.exe` if not already present (used to embed the app icon)
3. Bundle everything into `dist/flowcode-agent.exe` using [pkg](https://github.com/vercel/pkg)
4. Patch the PE subsystem to GUI (no console window)
5. Output the final versioned file as `dist/flowcode-agent-vX.X.X.exe`

> **Note:** The build targets Windows x64 (`node18-win-x64`). Run the build on Windows.
