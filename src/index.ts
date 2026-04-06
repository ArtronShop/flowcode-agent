import * as http from "http";
import { execFile } from "child_process";
import { WebSocketServer, WebSocket } from "ws";
import AutoLaunch from "auto-launch";
import { load_configs, get_configs, CONFIG_FILE, CONFIG_DIR } from "./configs";
import os from 'node:os';
import fs from 'node:fs';
import {
    arduino_dir_init,
    arduino_check_and_install_core,
    arduino_check_and_install_library,
    board_list,
    board_listall,
    compile,
    upload,
    version,
    sketch_create,
    sketch_read,
    sketch_write,
    port_list,
    port_connect,
    port_disconnect,
    port_write,
    sketch_delete,
    sketch_list,
    type OnData,
} from "./arduino";
import SysTray from 'systray2';
import { exit } from 'node:process';
import path from "node:path";

// ── load configs on startup ────────────────────────────────────────────────

load_configs();

const autoLaunch = new AutoLaunch({ name: "FlowcodeAgent", path: process.execPath });

async function apply_auto_start() {
    const enabled = get_configs().auto_start;
    const isEnabled = await autoLaunch.isEnabled();
    if (enabled && !isEnabled) {
        await autoLaunch.enable();
        console.log("Auto-start enabled");
    } else if (!enabled && isEnabled) {
        await autoLaunch.disable();
        console.log("Auto-start disabled");
    }
}

apply_auto_start().catch(err => console.error("auto-launch error:", err));

function open_settings_and_reload() {
    const notepad = execFile("notepad.exe", [CONFIG_FILE]);
    notepad.on("close", () => {
        load_configs();
        console.log("Configs reloaded:", CONFIG_FILE);
        apply_auto_start().catch(err => console.error("auto-launch error:", err));
    });
}

// ── HTTP server (CORS allow all) ───────────────────────────────────────────

const server = http.createServer((req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
    res.setHeader("Access-Control-Allow-Headers", "Content-Type");

    if (req.method === "OPTIONS") {
        res.writeHead(204);
        res.end();
        return;
    }

    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "flowcode-agent running" }));
});

// ── WebSocket server ───────────────────────────────────────────────────────

const wss = new WebSocketServer({
    server,
    // อนุญาตทุก origin
    verifyClient: () => true,
});

function send(ws: WebSocket, id: string | undefined, type: string, payload: any) {
    if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ id, type, payload }));
    }
}

function sendError(ws: WebSocket, id: string | undefined, message: string) {
    send(ws, id, "error", message);
}

function makeOnData(ws: WebSocket, id: string | undefined): OnData {
    return ({ stream, data }) => send(ws, id, "stream", { stream, data });
}

// ── message handler ────────────────────────────────────────────────────────

wss.on("connection", (ws, req) => {
    console.log(`[ws] connected from ${req.socket.remoteAddress}`);

    ws.on("message", async (raw) => {
        let msg: { id?: string; action: string; params?: any };

        try {
            msg = JSON.parse(raw.toString());
        } catch {
            sendError(ws, undefined, "invalid JSON");
            return;
        }

        const { id, action, params = {} } = msg;

        try {
            let result: any = null;

            switch (action) {
                // ── board ──────────────────────────────────────────────
                case "board.list":
                    result = await board_list();
                    break;

                case "board.listall":
                    result = await board_listall(params.fqbn);
                    break;

                // ── install ────────────────────────────────────────────
                case "core.install":
                    // params: { id, version, package_index } }
                    await arduino_check_and_install_core(params.id, params.version, params.package_index, makeOnData(ws, id));
                    result = { ok: true };
                    break;

                case "lib.install":
                    // params: { depends: string[] }
                    await arduino_check_and_install_library(params.depends, makeOnData(ws, id));
                    break;

                // ── sketch ────────────────────────────────────────────
                case "sketch.list":
                    result = sketch_list();
                    break;

                case "sketch.create":
                    // params: { name, code? }
                    result = { path: sketch_create(params.name, params.code ?? "") };
                    break;

                case "sketch.read":
                    // params: { name }
                    result = { code: sketch_read(params.name) };
                    break;

                case "sketch.write":
                    // params: { name, code }
                    sketch_write(params.name, params.code);
                    result = { ok: true };
                    break;

                case "sketch.delete":
                    // params: { name }
                    sketch_delete(params.name);
                    result = { ok: true };
                    break;

                // ── compile / upload ───────────────────────────────────
                case "compile":
                    // params: { sketch, fqbn, boardOption? }
                    await compile(params.sketch, params.fqbn, params.boardOption, makeOnData(ws, id));
                    result = { ok: true };
                    break;

                case "upload":
                    // params: { sketchName, fqbn, port, boardOption? }
                    await upload(params.sketch, params.fqbn, params.port, params.boardOption, makeOnData(ws, id));
                    result = { ok: true };
                    break;

                // ── serial port ───────────────────────────────────────
                case "port.list":
                    result = await port_list();
                    break;

                case "port.connect":
                    // params: { port, baudRate }
                    await port_connect(params.port, params.baudRate ?? 9600, (data) => {
                        send(ws, id, "port.data", { port: params.port, data });
                    }, () => {
                        send(ws, id, "port.close", { port: params.port });
                    });
                    result = { ok: true };
                    break;

                case "port.disconnect":
                    // params: { port }
                    await port_disconnect(params.port);
                    result = { ok: true };
                    break;

                case "port.write":
                    // params: { port, data }
                    await port_write(params.port, params.data);
                    result = { ok: true };
                    break;

                // ── misc ───────────────────────────────────────────────
                case "version":
                    result = await version();
                    break;

                case "config.init":
                    // params: { additional_urls?: string[] }
                    arduino_dir_init(params.additional_urls ?? []);
                    result = { ok: true };
                    break;

                default:
                    sendError(ws, id, `unknown action: ${action}`);
                    return;
            }

            send(ws, id, "result", result);
        } catch (err: any) {
            console.error(`[ws] action "${action}" error:`, err);
            sendError(ws, id, err?.message ?? String(err));
        }
    });

    ws.on("close", () => {
        console.log(`[ws] disconnected`);
    });
});

// ── start ──────────────────────────────────────────────────────────────────

const PORT = Number(process.env.PORT) || 8080;
server.listen(PORT, () => {
    console.log(`FlowCode Agent listening on http://0.0.0.0:${PORT}`);
});

// ── Extract Assets ─────────────────────────────────────────────────────────

const ASSET_DIR = path.join(CONFIG_DIR, "asset");

function extractAssets() {
    if (!fs.existsSync(ASSET_DIR)) {
        fs.mkdirSync(ASSET_DIR, { recursive: true });
    }

    const filesToExtract = ["logo.ico", "logo.png"];

    for (const fileName of filesToExtract) {
        const targetPath = path.join(ASSET_DIR, fileName);
        if (!fs.existsSync(targetPath)) {
            // dev: __dirname = src/  → up 1
            // build/pkg: __dirname = dist/src/ → up 2
            const internalPath = __dirname.includes(`${path.sep}dist${path.sep}`)
                ? path.join(__dirname, "..", "..", "asset", fileName)
                : path.join(__dirname, "..", "asset", fileName);
            try {
                fs.writeFileSync(targetPath, fs.readFileSync(internalPath));
                console.log(`Extracted: ${fileName} → ${targetPath}`);
            } catch (err: any) {
                console.error(`Failed to extract ${fileName}:`, err.message);
            }
        }
    }
}

extractAssets();

// ── Tray ───────────────────────────────────────────────────────────────────

const ICON_PATH = path.join(ASSET_DIR, os.platform() === "win32" ? "logo.ico" : "logo.png");

const systray = new SysTray({
  menu: {
    icon: ICON_PATH,
    title: 'FlowCode Agent',
    tooltip: 'FlowCode Agent',
    items: [
      {
        title: 'Settings...',
        tooltip: 'Open settings file to edit',
        checked: false,
        enabled: true,
      },
      {
        title: 'Exit',
        tooltip: 'Exit from the app.',
        checked: false,
        enabled: true,
      },
    ],
  },
  debug: false,
  copyDir: true, // copy go tray binary to outside directory, useful for packing tool like pkg.
});

systray.ready().then(() => {
  systray.onClick((event) => {
    if (event.seq_id === 0) {
      open_settings_and_reload();
    } else if (event.seq_id === 1) {
      systray.kill();
      console.log("Exit");
      exit();
    }
  });
}).catch(err => {
  console.log('systray failed to start: ' + err.message)
})

