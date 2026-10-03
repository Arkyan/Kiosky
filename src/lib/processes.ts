/** Ce que font les processus de Windows qu'on croise dans le Moniteur, et s'il faut s'en inquiéter. */
const KNOWN: Record<string, string> = {
  "memory compression":
    "Compression de la mémoire : Windows y range, compressées, les pages peu utilisées plutôt que de les écrire sur le disque. Plusieurs Go sont normaux quand beaucoup d'applications sont ouvertes.",
  system:
    "Le noyau de Windows et les pilotes. Une forte utilisation durable vient en général d'un pilote ou d'un périphérique.",
  registry: "Le registre de Windows gardé en mémoire, pour y accéder plus vite.",
  "secure system": "Zone isolée par la virtualisation, qui protège les identifiants et le noyau.",
  "svchost.exe":
    "Hôte de services Windows : chaque exemplaire fait tourner un ou plusieurs services (réseau, mises à jour, audio…). En avoir des dizaines est normal.",
  "dwm.exe":
    "Gestionnaire de fenêtres : il dessine le bureau, les fenêtres et leurs effets. Il utilise la carte graphique en permanence.",
  "csrss.exe": "Processus essentiel de Windows (fenêtres, consoles, arrêt). L'arrêter fait planter le système.",
  "smss.exe": "Gestionnaire de sessions, lancé au démarrage de Windows.",
  "wininit.exe": "Démarre les services essentiels au lancement de Windows.",
  "winlogon.exe": "Gère l'ouverture de session, le verrouillage et Ctrl+Alt+Suppr.",
  "services.exe": "Lance et arrête les services Windows.",
  "lsass.exe": "Vérifie les mots de passe et gère la sécurité des sessions.",
  "explorer.exe": "L'Explorateur de fichiers, mais aussi la barre des tâches et le bureau.",
  "msmpeng.exe":
    "Antivirus Microsoft Defender. Il travaille beaucoup pendant une analyse, ou quand de nombreux fichiers changent (compilation, npm install).",
  "nissrv.exe": "Protection réseau de Microsoft Defender.",
  "mpdefendercoreservice.exe": "Service principal de Microsoft Defender.",
  "searchindexer.exe": "Indexe les fichiers pour la recherche Windows. Actif après de gros changements de fichiers.",
  "searchhost.exe": "La recherche du menu Démarrer.",
  "startmenuexperiencehost.exe": "Le menu Démarrer.",
  "shellexperiencehost.exe": "Éléments de l'interface de Windows : centre de notifications, calendrier, menus.",
  "runtimebroker.exe": "Contrôle les autorisations des applications du Microsoft Store.",
  "audiodg.exe": "Moteur audio de Windows : mélange les sons et applique les effets.",
  "fontdrvhost.exe": "Affichage des polices, isolé du reste pour la sécurité.",
  "spoolsv.exe": "File d'attente des impressions.",
  "wmiprvse.exe": "Fournit des informations sur le système aux logiciels qui en demandent (WMI).",
  "tiworker.exe": "Installe les mises à jour de Windows. Gourmand pendant une installation, puis il s'arrête.",
  "trustedinstaller.exe": "Installe les mises à jour et les composants de Windows.",
  "conhost.exe": "Fenêtre de console : un exemplaire par programme en ligne de commande ouvert.",
  "ctfmon.exe": "Saisie de texte : claviers, saisie tactile, reconnaissance d'écriture.",
  "sihost.exe": "Infrastructure de l'interface : notifications, menu Démarrer, barre des tâches.",
  "taskhostw.exe": "Fait tourner des tâches planifiées de Windows.",
  "dllhost.exe": "Héberge des composants de Windows ou d'applications (miniatures, extensions…).",
  "wudfhost.exe": "Fait tourner des pilotes de périphériques hors du noyau (USB, capteurs…).",
  "vmmem":
    "Mémoire et processeur des machines virtuelles (WSL, Docker, Hyper-V). Pour la réduire : arrête les conteneurs ou lance « wsl --shutdown ».",
  "vmmemwsl":
    "Mémoire et processeur de WSL et des conteneurs Docker. Pour la réduire : arrête les conteneurs ou lance « wsl --shutdown ».",
  "msedgewebview2.exe":
    "Moteur web utilisé par des applications pour afficher leur interface (Kiosky, Teams, widgets de Windows…).",
};

/** Explication d'un processus connu, d'après le nom de son exécutable. */
export const explainProcess = (name: string): string | undefined => KNOWN[name.toLowerCase()];
