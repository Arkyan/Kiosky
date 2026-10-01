import { invoke } from "@tauri-apps/api/core";

export type ConvResult = { title: string; value: string; copy: string; hint: string; error: boolean; action: string };

export type Snippet = { trigger: string; text: string };
export type PresetRule = { key: string; volume: number; muted: boolean };
export type VolumePreset = { name: string; rules: PresetRule[] };
export type Settings = {
  palette_shortcut: string;
  expander_enabled: boolean;
  snippets: Snippet[];
  presets: VolumePreset[];
  picker_shortcut: string;
  color_format: ColorFormat;
  colors: string[];
  monitor_tooltip: boolean;
  monitor_tray_icon: boolean;
  cleaned_total: number;
  ports_hide_system: boolean;
  clean_folder_names: string[];
  clean_roots: string[];
  folder_shortcuts: FolderShortcut[];
  disabled_modules: string[];
  palette_disabled: string[];
  widget_enabled: boolean;
  widget_items: string[];
  widget_mode: "free" | "taskbar-left" | "taskbar-right";
  widget_pos: [number, number] | null;
  widget_vertical: boolean;
  widget_opacity: number;
  project_roots: string[];
  project_favorites: string[];
  project_opened: Record<string, number>;
};
export type Project = {
  name: string;
  path: string;
  tags: string[];
  git: boolean;
  branch: string | null;
  modified: number;
  editor: string;
};
export type GitStatus = { path: string; branch: string | null; changes: number; ahead: number; behind: number };
export type EnvVar = { name: string; value: string; expand: boolean };
export type EnvState = { user: EnvVar[]; machine: EnvVar[]; is_admin: boolean; undo: string | null };
export type PathCheck = { entry: string; expanded: string; exists: boolean };
export type ShellOutput = { output: string; code: number; ms: number; timed_out: boolean };
export type Distro = { name: string; running: boolean; version: number; default: boolean };
export type WslState = { installed: boolean; distros: Distro[] };
export type Container = {
  id: string;
  name: string;
  image: string;
  running: boolean;
  status: string;
  ports: number[];
  project: string | null;
};
export type DockerState = {
  installed: boolean;
  running: boolean;
  containers: Container[];
  desktop_path: string | null;
  error: string | null;
};
export type ScanProgress = { dirs: number; found: number };
export type FolderShortcut = { name: string; path: string; shortcut: string; open_with: string };
export type KnownFolder = { name: string; path: string };
export type Opener = { id: string; name: string; kind: "explorer" | "terminal" | "editor" };
export type ColorFormat = "hex" | "rgb" | "hsl";

export type PickerFrame = { grid: number; pixels: number[]; x: number; y: number };

export type CleanCategory = {
  id: string;
  name: string;
  description: string;
  size: number;
  files: number;
  needs_admin: boolean;
  available: boolean;
  recommended: boolean;
};
export type CleanScan = { categories: CleanCategory[]; is_admin: boolean };
export type CleanReport = { freed: number; deleted: number; skipped: number; kept: string[] };
export type SearchProgress = { phase: "search" | "measure"; dirs: number; found: number; measured: number };

export type PortEntry = {
  proto: "TCP" | "UDP";
  local_addr: string;
  local_port: number;
  remote_addr: string;
  remote_port: number;
  state: string;
  listening: boolean;
  pid: number;
  process: string;
  path: string;
  system: boolean;
};

export type FoundFolder = {
  path: string;
  name: string;
  parent: string;
  size: number;
  files: number;
  modified: number;
};

export type Sample = { cpu: number; mem_used: number; mem_total: number; net_down: number; net_up: number };
export type Disk = { letter: string; used: number; total: number };
export type ProcGroup = { name: string; pids: number[]; cpu: number; mem: number; system: boolean };
export type TopProcs = { cpu: ProcGroup[]; mem: ProcGroup[] };
export type MonitorState = { history: Sample[]; disks: Disk[]; top: TopProcs };

export type AudioApp = {
  key: string;
  name: string;
  pids: number[];
  volume: number;
  muted: boolean;
  active: boolean;
};
export type MixerState = { master: number; master_muted: boolean; apps: AudioApp[] };

export type StartupItem = {
  id: string;
  name: string;
  command: string;
  source: string;
  kind: "registry" | "folder" | "task";
  enabled: boolean;
  needs_admin: boolean;
  impact_ms: number | null;
  exe: string;
};
export type BootEntry = { date: string; total_ms: number; main_ms: number; post_ms: number };
export type StartupReport = {
  items: StartupItem[];
  is_admin: boolean;
  boot_history: BootEntry[];
  perf_available: boolean;
  tasks_error: string | null;
};

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  convert: (input: string) => invoke<ConvResult[]>("convert", { input }),
  hidePalette: () => invoke<void>("hide_palette"),

  getMixer: () => invoke<MixerState>("get_mixer"),
  setMasterVolume: (volume: number) => invoke<void>("set_master_volume", { volume }),
  setMasterMute: (muted: boolean) => invoke<void>("set_master_mute", { muted }),
  setAppVolume: (key: string, volume: number) => invoke<void>("set_app_volume", { key, volume }),
  setAppMute: (key: string, muted: boolean) => invoke<void>("set_app_mute", { key, muted }),
  applyPreset: (name: string) => invoke<void>("apply_preset", { name }),

  getStartup: () => invoke<StartupReport>("get_startup"),
  setStartupEnabled: (id: string, enabled: boolean) =>
    invoke<void>("set_startup_enabled", { id, enabled }),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),

  pickColor: () => invoke<void>("pick_color"),

  scanCleanup: () => invoke<CleanScan>("scan_cleanup"),
  runCleanup: (ids: string[]) => invoke<CleanReport>("run_cleanup", { ids }),
  findFolders: () => invoke<FoundFolder[]>("find_folders"),
  deleteFolders: (paths: string[]) => invoke<CleanReport>("delete_folders", { paths }),
  pickFolder: () => invoke<string | null>("pick_folder"),

  getPorts: () => invoke<PortEntry[]>("get_ports"),
  killProcess: (pid: number) => invoke<void>("kill_process", { pid }),

  getMonitor: () => invoke<MonitorState>("get_monitor"),
  killProcesses: (pids: number[]) => invoke<void>("kill_processes", { pids }),
  getOpeners: () => invoke<Opener[]>("get_openers"),
  getKnownFolders: () => invoke<KnownFolder[]>("get_known_folders"),
  getProjects: () => invoke<Project[]>("get_projects"),
  scanProjects: () => invoke<Project[]>("scan_projects"),
  gitStatus: (paths: string[]) => invoke<GitStatus[]>("git_status", { paths }),
  openProject: (path: string, opener: string) => invoke<void>("open_project", { path, opener }),
  getEnv: () => invoke<EnvState>("get_env"),
  setEnv: (machine: boolean, name: string, value: string) => invoke<void>("set_env", { machine, name, value }),
  deleteEnv: (machine: boolean, name: string) => invoke<void>("delete_env", { machine, name }),
  undoEnv: () => invoke<string>("undo_env"),
  checkPaths: (entries: string[]) => invoke<PathCheck[]>("check_paths", { entries }),
  openWith: (id: string, path: string) => invoke<void>("open_with", { id, path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  runAction: (action: string) => invoke<void>("run_action", { action }),
  paletteHome: () => invoke<ConvResult[]>("palette_home"),
  getWsl: () => invoke<WslState>("get_wsl"),
  wslAction: (name: string, action: string) => invoke<void>("wsl_action", { name, action }),
  getDocker: () => invoke<DockerState>("get_docker"),
  dockerAction: (ids: string[], action: string) => invoke<void>("docker_action", { ids, action }),
  dockerLogs: (id: string) => invoke<string>("docker_logs", { id }),
  startDockerDesktop: () => invoke<void>("start_docker_desktop"),
  fitWidget: (width: number, height: number) => invoke<void>("fit_widget", { width, height }),
  taskbarHeight: () => invoke<number | null>("taskbar_height"),
  getBattery: () => invoke<{ present: boolean; percent: number; charging: boolean }>("get_battery"),
  runShell: (cmd: string) => invoke<ShellOutput>("run_shell", { cmd }),
  getIcons: (actions: string[]) => invoke<Record<string, string>>("get_icons", { actions }),

  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
};

/** Copie dans le presse-papiers, avec repli si l'API moderne est refusée. */
export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
}

export const fmtSeconds = (ms: number) =>
  (ms / 1000).toLocaleString("fr-FR", { minimumFractionDigits: 1, maximumFractionDigits: 1 }) + " s";

/** 1536 → « 1,5 Ko » */
export function fmtBytes(b: number): string {
  const units = ["o", "Ko", "Mo", "Go", "To"];
  let v = b;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const digits = i === 0 || v >= 100 ? 0 : 1;
  return v.toLocaleString("fr-FR", { maximumFractionDigits: digits }) + " " + units[i];
}

/** « #0067C0 » → texte dans le format voulu (même calcul que colorpicker.rs) */
export function formatColor(hex: string, format: ColorFormat): string {
  const n = parseInt(hex.slice(1), 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  if (format === "rgb") return `rgb(${r}, ${g}, ${b})`;
  if (format === "hsl") {
    const [rf, gf, bf] = [r / 255, g / 255, b / 255];
    const max = Math.max(rf, gf, bf);
    const min = Math.min(rf, gf, bf);
    const l = (max + min) / 2;
    const d = max - min;
    let h = 0;
    let s = 0;
    if (d !== 0) {
      s = d / (1 - Math.abs(2 * l - 1));
      h = max === rf ? ((gf - bf) / d + 6) % 6 : max === gf ? (bf - rf) / d + 2 : (rf - gf) / d + 4;
      h *= 60;
    }
    return `hsl(${Math.round(h)}, ${Math.round(s * 100)}%, ${Math.round(l * 100)}%)`;
  }
  return hex.toUpperCase();
}

/** Couleur de texte lisible sur un fond donné */
export function contrastOn(hex: string): string {
  const n = parseInt(hex.slice(1), 16);
  const lum = (0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) / 255;
  return lum > 0.6 ? "#000" : "#fff";
}
