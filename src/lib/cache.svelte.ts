import type { CleanScan, FoundFolder } from "./api";

/**
 * Résultats gardés en mémoire tant que l'application tourne : changer de page
 * (ou fermer la fenêtre, qui ne fait que la cacher) ne relance pas une recherche de 40 s.
 */
export const cleanCache = $state<{
  scan: CleanScan | null;
  checked: Record<string, boolean>;
  found: FoundFolder[] | null;
  foundAt: number;
  picked: Record<string, boolean>;
}>({
  scan: null,
  checked: {},
  found: null,
  foundAt: 0,
  picked: {},
});
