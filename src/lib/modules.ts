/** Modules de Kiosky : barre latérale et section « Modules » des réglages. */
export type ModuleId =
  | "converter"
  | "expander"
  | "color"
  | "mixer"
  | "projects"
  | "folders"
  | "env"
  | "containers"
  | "monitor"
  | "startup"
  | "cleaner"
  | "updates"
  | "ports"
  | "network"
  | "ssh"
  | "devtools"
  | "locks";

export type ModuleInfo = { id: ModuleId; label: string; icon: string; group: string; description: string };

export const MODULES: ModuleInfo[] = [
  { id: "converter", label: "Convertisseur", icon: "calc", group: "Outils", description: "Calculs et conversions (toujours actif : c'est aussi la palette)" },
  { id: "expander", label: "Expanseur de texte", icon: "keyboard", group: "Outils", description: "Déclencheurs remplacés par du texte (coupe le hook clavier)" },
  { id: "color", label: "Pipette", icon: "pipette", group: "Outils", description: "Raccourci, menu de l'icône et historique des couleurs" },
  { id: "mixer", label: "Volume", icon: "volume", group: "Outils", description: "Volume par application et préréglages" },
  { id: "projects", label: "Projets", icon: "code", group: "Dev", description: "Lanceur de projets, aussi dans la palette" },
  { id: "folders", label: "Dossiers", icon: "folder", group: "Dev", description: "Raccourcis clavier vers les dossiers favoris" },
  { id: "env", label: "Variables", icon: "variable", group: "Dev", description: "PATH et variables d'environnement" },
  { id: "containers", label: "Conteneurs", icon: "box", group: "Dev", description: "WSL et Docker" },
  { id: "ssh", label: "SSH", icon: "key", group: "Dev", description: "Serveurs du fichier ~/.ssh/config et clés, aussi dans la palette" },
  { id: "devtools", label: "Outils dev", icon: "terminal", group: "Dev", description: "Node, Python, Rust, Git… installés, leur version et leur emplacement" },
  { id: "monitor", label: "Moniteur", icon: "activity", group: "Système", description: "Graphiques, infobulle et jauge CPU de l'icône" },
  { id: "startup", label: "Démarrage", icon: "power", group: "Système", description: "Programmes lancés avec Windows" },
  { id: "cleaner", label: "Nettoyage", icon: "broom", group: "Système", description: "Fichiers temporaires, caches, node_modules…" },
  { id: "updates", label: "Mises à jour", icon: "download", group: "Système", description: "Nouvelles versions des applications installées (winget)" },
  { id: "ports", label: "Ports", icon: "plug", group: "Système", description: "Ports ouverts, et kill 3000 dans la palette" },
  { id: "locks", label: "Fichiers bloqués", icon: "lock", group: "Système", description: "Quel programme utilise un fichier ou un dossier" },
  { id: "network", label: "Réseau", icon: "wifi", group: "Système", description: "Adresses IP, DNS, test de débit et fichier hosts" },
];

/** Ce que la palette peut chercher (désactivable une par une). */
export const PALETTE_SOURCES: { id: string; label: string; icon: string; description: string }[] = [
  { id: "apps", label: "Applications", icon: "sparkle", description: "Programmes et applis du Microsoft Store" },
  { id: "projects", label: "Projets", icon: "code", description: "Ouvrir un projet dans son éditeur" },
  { id: "folders", label: "Dossiers favoris", icon: "folder", description: "Les dossiers de la page Dossiers" },
  { id: "settings", label: "Paramètres Windows", icon: "settings", description: "Wi-Fi, Bluetooth, affichage, mises à jour…" },
  { id: "tools", label: "Outils système", icon: "terminal", description: "Gestionnaire des tâches, services, registre…" },
  { id: "system", label: "Actions système", icon: "power", description: "Verrouiller, veille, redémarrer, éteindre" },
  { id: "toolbox", label: "Pages de Kiosky", icon: "calc", description: "Ouvrir une page, prendre une couleur" },
  { id: "calc", label: "Calculs et conversions", icon: "calc", description: "2+2, 10 km en miles, 50 eur usd, heures…" },
  { id: "web", label: "Recherche web", icon: "globe", description: "g, yt, gh, mdn… et « Rechercher sur Google »" },
  { id: "shell", label: "Commandes", icon: "terminal", description: ">ipconfig : exécuter une commande PowerShell" },
  { id: "ai", label: "Assistant", icon: "chat", description: "? ta question : une réponse de Claude ou Gemini" },
];
