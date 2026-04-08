import * as path from "path";

import * as fs from "fs";
import { spawn } from "child_process";
import { SerialPort } from "serialport";
import { get_configs, get_additional_urls_from_preferences } from "./configs";

// ── dynamic paths from configs ─────────────────────────────────────────────

export const get_sketch_path  = () => get_configs().arduino_sketch_path;
export const get_data_path    = () => get_configs().arduino_data_path;
export const get_download_path = () => get_configs().arduino_downloads_path;
export const get_cli_path     = () => get_configs().arduino_cli_path;
export const get_config_file  = () => path.join(get_data_path(), "settings.yaml");
const CLI = () => `"${get_cli_path()}" --config-file "${get_config_file()}"`;

// legacy exports (backward compat)
export const ARDUINO_SKETCH_PATH  = new Proxy({} as any, { get: () => get_sketch_path() });
export const ARDUINO_DATA_PATH    = new Proxy({} as any, { get: () => get_data_path() });
export const ARDUINO_DOWNLOAD_PATH = new Proxy({} as any, { get: () => get_download_path() });
export const ARDUINO_CLI_PATH     = new Proxy({} as any, { get: () => get_cli_path() });
export const ARDUINO_CONFIG_FILE  = new Proxy({} as any, { get: () => get_config_file() });

// ── config init ────────────────────────────────────────────────────────────

export function arduino_dir_init(add_additional_urls: string[] = []) {
    const sketchPath = get_sketch_path();
    const dataPath   = get_data_path();
    const dlPath     = get_download_path();

    for (const dir of [dataPath, dlPath, sketchPath]) {
        if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
    }

    const prefs_urls = get_additional_urls_from_preferences();
    const all_urls = [...prefs_urls, ...add_additional_urls.filter(u => !prefs_urls.includes(u))];
    const urls_yaml = all_urls.length === 0 ? " []" : "\n" + all_urls.map(u => `    - ${u}`).join("\n");

    try {
        fs.writeFileSync(get_config_file(),
`directories:
  data: ${JSON.stringify(dataPath)}
  downloads: ${JSON.stringify(dlPath)}
  user: ${JSON.stringify(sketchPath)}
board_manager:
  additional_urls:${urls_yaml}
updater:
  enable_notification: false
`);
    } catch (err) {
        console.error("write arduino config fail", err);
    }
}

// ── runner ─────────────────────────────────────────────────────────────────

export type OnData = (chunk: { stream: "stdout" | "stderr"; data: string }) => void;

async function run(cmd: string, onData?: OnData): Promise<{ stdout: string; stderr: string; out_json: any }> {
    console.log(cmd);
    return new Promise((resolve, reject) => {
        const proc = spawn(cmd, [], { shell: true, windowsHide: true });
        let stdout = "", stderr = "";
        proc.stdout.on("data", d => {
            const data = d.toString();
            stdout += data;
            console.log("stdout:", data);
            onData?.({ stream: "stdout", data });
        });
        proc.stderr.on("data", d => {
            const data = d.toString();
            stderr += data;
            console.warn("stderr:", data);
            onData?.({ stream: "stderr", data });
        });
        proc.on("exit", code => {
            if (code === 0) {
                let out_json: any = null;
                if (cmd.includes("--format jsonmini")) {
                    try { out_json = JSON.parse(stdout); } catch { console.warn("parse json fail", stdout); }
                }
                resolve({ stdout, stderr, out_json });
            } else {
                reject({ stdout, stderr });
            }
        });
    });
}

// ── board ──────────────────────────────────────────────────────────────────

export async function board_attach(port: string, fqbn: string) {
    await run(`${CLI()} board attach -p ${port} -b ${fqbn}`);
}

export async function board_details(fqbn: string) {
    const { out_json } = await run(`${CLI()} board details -b ${fqbn} --format jsonmini`);
    return out_json;
}

export async function board_list() {
    const { out_json } = await run(`${CLI()} board list --format jsonmini`);
    return out_json;
}

export async function board_listall(fqbn?: string) {
    const { out_json } = await run(`${CLI()} board listall ${fqbn ?? ""} --format jsonmini`);
    return out_json;
}

export async function board_search(query: string) {
    const { out_json } = await run(`${CLI()} board search ${query} --format jsonmini`);
    return out_json;
}

// ── burn-bootloader ────────────────────────────────────────────────────────

export async function burn_bootloader(fqbn: string, port: string, programmer: string) {
    await run(`${CLI()} burn-bootloader -b ${fqbn} -p ${port} -P ${programmer}`);
}

// ── cache ──────────────────────────────────────────────────────────────────

export async function cache_clean() {
    await run(`${CLI()} cache clean`);
}

// ── sketch file helpers ────────────────────────────────────────────────────

export function sketch_dir(sketchName: string) {
    return path.join(get_sketch_path(), sketchName);
}

export function sketch_ino_path(sketchName: string) {
    return path.join(sketch_dir(sketchName), `${sketchName}.ino`);
}

export function sketch_create(sketchName: string, code = "") {
    const dir = sketch_dir(sketchName);
    fs.mkdirSync(dir, { recursive: true });
    const inoPath = sketch_ino_path(sketchName);
    if (!fs.existsSync(inoPath)) {
        fs.writeFileSync(inoPath, code, { encoding: "utf8" });
    }
    return inoPath;
}

export function sketch_read(sketchName: string): string {
    return fs.readFileSync(sketch_ino_path(sketchName), { encoding: "utf8" });
}

export function sketch_write(sketchName: string, code: string) {
    const dir = sketch_dir(sketchName);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(sketch_ino_path(sketchName), code, { encoding: "utf8" });
}

export function sketch_delete(sketchName: string) {
    const dir = sketch_dir(sketchName);
    if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
}

export function sketch_list(): string[] {
    if (!fs.existsSync(ARDUINO_SKETCH_PATH)) return [];
    return fs.readdirSync(ARDUINO_SKETCH_PATH, { withFileTypes: true })
        .filter(e => e.isDirectory() && fs.existsSync(path.join(ARDUINO_SKETCH_PATH, e.name, `${e.name}.ino`)))
        .map(e => e.name);
}

// ── compile ────────────────────────────────────────────────────────────────

export async function compile(sketch: string, fqbn: string, boardOption?: string, onData?: OnData) {
    const opt = boardOption ? ` --board-options "${boardOption}"` : "";
    await run(`${CLI()} compile -b ${fqbn}${opt} "${sketch_dir(sketch)}" -v`, onData);
}

// ── config ─────────────────────────────────────────────────────────────────

export async function config_dump() {
    const { out_json } = await run(`${CLI()} config dump --format jsonmini`);
    return out_json;
}

export async function config_init() {
    await run(`${CLI()} config init`);
}

export async function config_add(key: string, value: string) {
    await run(`${CLI()} config add ${key} ${value}`);
}

export async function config_delete(key: string) {
    await run(`${CLI()} config delete ${key}`);
}

export async function config_remove(key: string, value: string) {
    await run(`${CLI()} config remove ${key} ${value}`);
}

export async function config_set(key: string, value: string) {
    await run(`${CLI()} config set ${key} ${value}`);
}

// ── core ───────────────────────────────────────────────────────────────────

export async function core_download(id: string) {
    await run(`${CLI()} core download ${id}`);
}

export async function core_install(id: string, onData?: OnData) {
    await run(`${CLI()} core install ${id}`, onData);
}

export async function core_list() {
    const { out_json } = await run(`${CLI()} core list --format jsonmini`);
    return out_json;
}

export async function core_search(query: string) {
    const { out_json } = await run(`${CLI()} core search ${query} --format jsonmini`);
    return out_json;
}

export async function core_uninstall(id: string) {
    await run(`${CLI()} core uninstall ${id}`);
}

export async function core_update_index(onData?: OnData) {
    await run(`${CLI()} core update-index`, onData);
}

export async function core_upgrade(id?: string, onData?: OnData) {
    await run(`${CLI()} core upgrade ${id ?? ""}`, onData);
}

// ── lib ────────────────────────────────────────────────────────────────────

export async function lib_deps(name: string) {
    const { out_json } = await run(`${CLI()} lib deps ${name} --format jsonmini`);
    return out_json;
}

export async function lib_download(name: string) {
    await run(`${CLI()} lib download "${name}"`);
}

export async function lib_examples(name: string) {
    const { out_json } = await run(`${CLI()} lib examples ${name} --format jsonmini`);
    return out_json;
}

export async function lib_install(name: string, onData?: OnData) {
    await run(`${CLI()} lib install "${name}"`, onData);
}

export async function lib_list() {
    const { out_json } = await run(`${CLI()} lib list --format jsonmini`);
    return out_json?.installed_libraries?.map((a: any) => a.library) ?? [];
}

export async function lib_search(query: string) {
    const { out_json } = await run(`${CLI()} lib search "${query}" --format jsonmini`);
    return out_json;
}

export async function lib_uninstall(name: string) {
    await run(`${CLI()} lib uninstall "${name}"`);
}

export async function lib_update_index(onData?: OnData) {
    await run(`${CLI()} lib update-index`, onData);
}

export async function lib_upgrade(name?: string) {
    await run(`${CLI()} lib upgrade ${name ?? ""}`);
}

// ── monitor ────────────────────────────────────────────────────────────────

export async function monitor(port: string, fqbn: string) {
    await run(`${CLI()} monitor -p ${port} -b ${fqbn}`);
}

// ── outdated ───────────────────────────────────────────────────────────────

export async function outdated() {
    const { out_json } = await run(`${CLI()} outdated --format jsonmini`);
    return out_json;
}

// ── sketch ─────────────────────────────────────────────────────────────────

export async function sketch_archive(sketchPath: string, archivePath?: string) {
    const dest = archivePath ? ` "${archivePath}"` : "";
    await run(`${CLI()} sketch archive "${sketchPath}"${dest}`);
}

export async function sketch_new(name: string) {
    await run(`${CLI()} sketch new "${name}"`);
}

// ── update / upgrade ───────────────────────────────────────────────────────

export async function update() {
    await run(`${CLI()} update`);
}

export async function upgrade() {
    await run(`${CLI()} upgrade`);
}

// ── upload ─────────────────────────────────────────────────────────────────

export async function upload(sketch: string, fqbn: string, port: string, boardOption?: string, onData?: OnData) {
    const opt = boardOption ? ` --board-options "${boardOption}"` : "";
    await run(`${CLI()} upload -b ${fqbn} -p ${port}${opt} "${sketch_dir(sketch)}" -v`, onData);
}

// ── version ────────────────────────────────────────────────────────────────

export async function version() {
    const { out_json } = await run(`${CLI()} version --format jsonmini`);
    return out_json;
}

// ── serial port ────────────────────────────────────────────────────────────

export type OnSerialData = (data: string) => void;
export type OnSerialClose = () => void;

const _ports = new Map<string, SerialPort>();

export async function port_list() {
    return SerialPort.list();
}

export async function port_connect(portPath: string, baudRate: number, onData: OnSerialData, onClose: OnSerialClose) {
    let sp: SerialPort;
    if (!_ports.has(portPath)) {
        sp = new SerialPort({ 
            path: portPath, 
            baudRate, 
            autoOpen: false,
        });
        await new Promise<void>((resolve, reject) => {
            sp.open(err => err ? reject(err) : resolve());
        });
    } else {
        sp = _ports.get(portPath);
        sp.update({ baudRate });
        sp.removeAllListeners();
    }
    sp.set({
        dtr: true,
        rts: true
    });
    sp.on("data", (chunk: Buffer) => onData(chunk.toString()));
    sp.on("close", () => {
        _ports.delete(portPath);
        onClose();
    });
    _ports.set(portPath, sp);
}

export async function port_disconnect(portPath: string) {
    const sp = _ports.get(portPath);
    if (!sp) return;
    await new Promise<void>((resolve, reject) => {
        sp.close(err => err ? reject(err) : resolve());
    });
    _ports.delete(portPath);
}

export async function port_write(portPath: string, data: string) {
    const sp = _ports.get(portPath);
    if (!sp) throw new Error(`Port ${portPath} not open`);
    await new Promise<void>((resolve, reject) => {
        sp.write(data, err => err ? reject(err) : resolve());
    });
}

// ── high-level helpers ─────────────────────────────────────────────────────

let _lib_index_updated = false;

export async function arduino_check_and_install_library(depends: string[], onData?: OnData) {
    const installed = (await lib_list()).map((a: any) => `${a.name}@${a.version}`);
    const to_install = depends.filter(d => !installed.includes(d));

    if (to_install.length > 0 && !_lib_index_updated) {
        await lib_update_index(onData);
        _lib_index_updated = true;
    }

    for (const name of to_install) {
        await lib_install(name, onData);
    }

    return true;
}

export async function arduino_check_and_install_core(id: string, version: string, package_index: string, onData?: OnData) {
    arduino_dir_init(package_index ? [package_index] : []);

    const installed = (await core_list())?.platforms?.some((b: any) => (b.id === id && b.installed_version === version));
    if (!installed) {
        // Update package index from local file & folder
        await core_update_index(onData);
        
        // Get core list after update
        const list = await core_list();
        const coreInfo = list?.platforms?.some((b: any) => b.id === id);
        if ((!coreInfo) || (coreInfo?.installed_version !== version)) {
            await core_install(id + '@' + version, onData);
        }
    }
}

export async function arduino_upload(sketchName: string, fqbn: string, port: string, boardOption?: string, onData?: OnData) {
    await compile(fqbn, sketchName, boardOption, onData);
    await upload(fqbn, port, sketchName, boardOption, onData);
}
