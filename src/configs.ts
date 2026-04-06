import * as fs from "fs";
import * as path from "path";
import * as os from "os";

export type ConfigsProps = {
    arduino_cli_path: string;
    arduino_data_path: string;
    arduino_downloads_path: string;
    arduino_sketch_path: string;
    auto_start: boolean;
    // preferences.txt
    arduino_preferences_path: string;
    arduino_sketch_path_from_preferences: boolean;
    additional_urls_from_preferences: boolean;
};

const arduin_cli_name: Record<string, string> = {
    darwin: "arduino-cli",
    linux: "arduino-cli-ubuntu-x64",
    win32: "arduino-cli.exe",
};

export const CONFIG_DIR = path.join(os.homedir(), "AppData", "Local", "FlowcodeAgent");
export const CONFIG_FILE = path.join(CONFIG_DIR, "configs.json");

const DEFAULT_PREFS_PATH = path.join(os.homedir(), "AppData", "Local", "Arduino15", "preferences.txt");

function read_prefs(prefsPath: string): Record<string, string> {
    try {
        if (fs.existsSync(prefsPath)) {
            const content = fs.readFileSync(prefsPath, "utf8");
            const result: Record<string, string> = {};
            for (const line of content.split(/\r?\n/)) {
                const eq = line.indexOf("=");
                if (eq > 0) result[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
            }
            return result;
        }
    } catch (err) {
        console.error("Error reading preferences.txt:", err);
    }
    return {};
}

const DEFAULTS: ConfigsProps = {
    arduino_cli_path: path.normalize(
        os.homedir() + "/AppData/Local/Programs/Arduino IDE/resources/app/lib/backend/resources/" + (arduin_cli_name[os.platform()] ?? "arduino-cli")
    ),
    arduino_data_path: path.join(os.homedir(), "AppData", "Local", "Arduino15"),
    arduino_downloads_path: path.join(os.homedir(), "AppData", "Local", "Arduino15", "staging"),
    arduino_sketch_path: path.join(os.homedir(), "Documents", "Arduino"),
    auto_start: true,
    arduino_preferences_path: DEFAULT_PREFS_PATH,
    arduino_sketch_path_from_preferences: true,
    additional_urls_from_preferences: true,
};

let _configs: ConfigsProps = { ...DEFAULTS };

export function load_configs(): ConfigsProps {
    try {
        if (!fs.existsSync(CONFIG_DIR)) {
            fs.mkdirSync(CONFIG_DIR, { recursive: true });
        }
        if (!fs.existsSync(CONFIG_FILE)) {
            fs.writeFileSync(CONFIG_FILE, JSON.stringify(DEFAULTS, null, 2), "utf8");
            _configs = { ...DEFAULTS };
        } else {
            const raw = fs.readFileSync(CONFIG_FILE, "utf8");
            _configs = { ...DEFAULTS, ...JSON.parse(raw) };
        }

        // override arduino_sketch_path from preferences.txt if enabled
        if (_configs.arduino_sketch_path_from_preferences) {
            const prefs = read_prefs(_configs.arduino_preferences_path);
            if (prefs["sketchbook.path"]) {
                _configs.arduino_sketch_path = prefs["sketchbook.path"];
            }
        }
    } catch (err) {
        console.error("Failed to load configs, using defaults:", err);
        _configs = { ...DEFAULTS };
    }
    return _configs;
}

export function get_configs(): ConfigsProps {
    return _configs;
}

/** อ่าน additional_urls จาก preferences.txt (ถ้าเปิดใช้งาน) */
export function get_additional_urls_from_preferences(): string[] {
    if (!_configs.additional_urls_from_preferences) return [];
    const prefs = read_prefs(_configs.arduino_preferences_path);
    const raw = prefs["boardsmanager.additional.urls"];
    if (!raw) return [];
    return raw.split(",").map(u => u.trim()).filter(Boolean);
}
