import { api, type Settings } from "./api";

/** Réglages partagés par toutes les pages (réactifs grâce à $state). */
export const store = $state<{ s: Settings | null; error: string; saved: boolean }>({
  s: null,
  error: "",
  saved: false,
});

export async function loadSettings() {
  store.s = await api.getSettings();
}

let timer: ReturnType<typeof setTimeout> | undefined;
let savedTimer: ReturnType<typeof setTimeout> | undefined;

/** Sauvegarde différée : plusieurs modifications rapprochées = une seule écriture. */
export function saveSettings(delay = 400): Promise<void> {
  clearTimeout(timer);
  return new Promise((resolve) => {
    timer = setTimeout(async () => {
      if (!store.s) return resolve();
      try {
        await api.saveSettings($state.snapshot(store.s) as Settings);
        store.error = "";
        store.saved = true;
        clearTimeout(savedTimer);
        savedTimer = setTimeout(() => (store.saved = false), 1500);
      } catch (e) {
        store.error = String(e);
        await loadSettings(); // on revient à l'état réellement appliqué
      }
      resolve();
    }, delay);
  });
}
