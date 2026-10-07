# Kiosky

Petite boîte à outils Windows qui vit dans la zone de notification.
Rust + Tauri 2 pour la logique, Svelte 5 pour l'interface (style Windows 11, Mica).

| Module | Ce qu'il fait |
|---|---|
| **Palette** | Raccourci global (`Ctrl+Shift+Space` par défaut) : recherche façon menu Démarrer (applications, projets, dossiers, Paramètres Windows, outils système, verrouiller/veille/redémarrer, pages de Kiosky) qui apprend de tes habitudes, plus calculs, unités, devises, fuseaux, encodages `kill 3000` / `port 3000`, `ip` (adresses locale et publique), le nom d'un serveur SSH pour s'y connecter (terminal ou VS Code) ou d'un projet SSH pour l'ouvrir directement dans son dossier, recherche web (`g`, `yt`, `gh`, `mdn`, `npm`, `crates`, `so`, `wiki`…) commandes `>ipconfig` avec la sortie affichée, et un assistant (`? ta question`, ou le bouton à gauche du champ) : Claude ou Gemini avec ta clé API, en conversation, avec des actions sur le texte copié (corriger, traduire, résumer, expliquer, reformuler) |
| **Expanseur de texte** | `;mail` → ton adresse, `;sig` → ta signature, `;date` → la date du jour, dans toutes les applications |
| **Pipette** | Raccourci global (`Win+Shift+C` par défaut) : loupe sous la souris, clic = couleur copiée en HEX, RGB ou HSL, historique |
| **Projets** | Détecte les projets (Git, Node, Rust, PHP, .NET, Android…), branche et modifications non commitées, ouverture dans l'éditeur adapté |
| **Dossiers** | Un raccourci global par dossier favori (`Ctrl+Shift+1`…), ouvert dans l'Explorateur, le terminal ou un éditeur |
| **SSH** | Serveurs du fichier `~/.ssh/config` : ajouter, modifier (adresse, utilisateur, port, clé, autres options), supprimer, se connecter dans un terminal ou ouvrir le serveur dans VS Code (extension Remote - SSH), annuler la dernière modification, oublier l'empreinte d'un serveur réinstallé. Clés : création (ED25519), copie de la clé publique, envoi sur un serveur. Projets : un dossier d'un serveur (`/root/citesco` sur `raildle`), ouvert directement dans un terminal ou VS Code, depuis la page ou la palette, en une seule connexion. Tunnels : un port du serveur amené sur `localhost`, ouverts et fermés d'un clic |
| **Outils dev** | Node, Python, Rust, Go, Java, Git, Docker… : ce qui est installé, la version, l'emplacement trouvé par le PATH, les exemplaires en double, et les mises à jour disponibles (winget, rustup) |
| **Variables** | PATH et variables utilisateur/système : chemins introuvables et doublons signalés, réordonnancement, annulation de la dernière modification |
| **Conteneurs** | Distributions WSL (terminal, fichiers, démarrer/arrêter, par défaut) et conteneurs Docker groupés par projet compose (démarrer, arrêter, journaux, terminal, ports), lancement de Docker Desktop |
| **Volume** | Volume et sourdine par application avec vu-mètres en direct, sortie par application (Spotify sur les enceintes, Discord dans le casque), micro (volume, niveau, coupure par raccourci `Ctrl+Alt+M`), préréglages |
| **Moniteur** | CPU, processeur graphique (utilisation et mémoire vidéo), mémoire et réseau en direct (2 min d'historique), espace disque, température de la carte graphique, processus les plus gourmands en CPU, GPU et mémoire (avec arrêt, et une explication pour ceux de Windows), résumé dans l'infobulle et jauge CPU comme icône |
| **Démarrage** | Programmes lancés avec Windows (registre, dossier Démarrage, tâches planifiées), activation/désactivation, temps de démarrage |
| **Nettoyage** | Temp, caches des navigateurs, rapports d'erreur, Windows Update, caches npm/pip, corbeille, avec la place gagnée. Recherche de dossiers par nom (`node_modules`, `target`, `.venv`…) dans les dossiers choisis |
| **Mises à jour** | Applications installées qui ont une nouvelle version (via winget), mise à jour une par une ou toutes d'un coup, applications à ignorer |
| **Ports** | Qui écoute sur quel port (« le port 3000 est-il libre ? »), processus propriétaire, bouton pour l'arrêter. Les ports de Windows peuvent être masqués |
| **Fichiers bloqués** | « Ce fichier est utilisé par un autre programme » : déposer un fichier ou un dossier pour voir quels programmes le tiennent (et quels fichiers), avec un bouton pour les arrêter |
| **Réseau** | Adresse locale et adresse publique (IPv4 et IPv6), cartes réseau (passerelle, DNS, MAC, débit de la liaison), test de débit (latence, descendant, montant), résolution d'un nom et vidage du cache DNS, fichier hosts (activer, désactiver, ajouter, annuler la dernière modification) |

**Barre flottante** (Réglages → Barre flottante) : petite barre toujours visible, posée sur la barre des tâches (à gauche ou à droite) ou déplaçable, avec les éléments choisis : CPU, GPU, RAM, réseau, heure, date, batterie, serveurs locaux, tunnels SSH ouverts, conteneurs Docker, mises à jour disponibles, musique en cours (pochette, titre, ⏮ ⏯ ⏭), volume (molette), voyant du micro. Clic sur un élément : son action ; survol : le détail, toujours affiché au-dessus de la barre. Elle se cache quand une application est en plein écran.

Chaque module peut être désactivé dans Réglages → Modules (il disparaît de l'interface et ne tourne plus en arrière-plan), et chaque source de la palette dans Réglages → Palette.

## Prérequis (une seule fois)

1. **Rust** : <https://rustup.rs> (choisis la toolchain `stable-x86_64-pc-windows-msvc`)
2. **Outils de build C++** : Visual Studio Build Tools avec « Développement Desktop en C++ »
3. **Node.js 20+** : <https://nodejs.org>
4. WebView2 est déjà présent sur Windows 11.

## Lancer en développement

```powershell
npm install
npm run tauri dev
```

La première compilation Rust prend quelques minutes, les suivantes quelques secondes.

## Construire l'installateur

```powershell
npm run tauri build
```

L'exe et les installateurs (`.msi`, `-setup.exe`) sont dans `src-tauri/target/release/bundle/`.

## Publier une version

Kiosky se met à jour tout seul (Réglages → Mises à jour) : il lit `latest.json` dans la dernière
release GitHub, télécharge l'installateur et vérifie sa signature.

1. Change la version dans `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` et `package.json`.
2. Lance le script, qui construit, signe et publie la release `v<version>` :

```powershell
.\scripts\release.ps1 -Publish -Notes "Ce qui change"
```

La clé privée de signature est dans `%USERPROFILE%\.tauri\kiosky.key`, hors du dépôt. Garde-en une copie :
sans elle, les versions déjà installées refuseront toutes les mises à jour suivantes.
Le dépôt (ou au moins ses releases) doit être public pour que l'application puisse les télécharger.

## Organisation

```
src/                     Interface Svelte
  pages/                 Une page par module
  lib/                   Composants partagés, appels à Rust (api.ts)
  Palette.svelte         Fenêtre flottante du convertisseur
  Picker.svelte          Loupe de la pipette
  Widget.svelte          Barre flottante
  Tip.svelte             Infobulle de la barre flottante
scripts/release.ps1      Construction signée, latest.json et release GitHub
src-tauri/src/
  main.rs                Fenêtres, zone de notification, raccourci, commandes
  search.rs              Recherche de la palette (Get-StartApps, score, usage)
  shell.rs               Commandes « > » de la palette (PowerShell, délai max 20 s)
  converter.rs           Moteurs de conversion (devises, heures, encodages…)
  units.rs / calc.rs     Unités et calculatrice (testés : `cargo test`)
  expander.rs            Hook clavier + saisie simulée
  audio.rs               Mixeur (API Core Audio, COM)
  projects.rs            Détection des projets, technos, git status
  envvars.rs             Variables d'environnement (registre), sauvegarde avant chaque écriture
  media.rs               Musique en cours (contrôles multimédias de Windows)
  containers.rs          WSL (wsl.exe) et Docker (CLI docker)
  apps.rs                Mises à jour des applications (winget)
  ai.rs                  Assistant de la palette (API Claude et Gemini, clés chiffrées par Windows)
  widget.rs              Barre flottante : placement sur la barre des tâches, premier plan (événements Windows), plein écran
  launcher.rs            « Ouvrir avec » : Explorateur, terminal, VS Code, JetBrains…
  colorpicker.rs         Pipette : hook souris + capture d'écran (GDI)
  monitor.rs             CPU / GPU / RAM / réseau / disques
  cleaner.rs             Nettoyage (ne suit jamais les liens ni les jonctions)
  ports.rs               Tables TCP/UDP (IP Helper) et arrêt de processus
  ssh.rs                 Fichier ~/.ssh/config (commentaires et mise en forme conservés), clés, connexion, projets distants, tunnels
  locks.rs               Programmes qui tiennent un fichier (Gestionnaire de redémarrage de Windows)
  devtools.rs            Outils de développement du PATH et leurs versions
  network.rs             Cartes réseau (IP Helper), adresse publique et test de débit (Cloudflare), DNS, fichier hosts
  startup.rs             Registre, dossiers Démarrage, tâches, journal de performances
  settings.rs            Réglages JSON dans %APPDATA%\com.kiosky.desktop
```

## Bon à savoir

- Fermer la fenêtre la **cache** seulement. Pour quitter : clic droit sur l'icône → Quitter.
- Les **temps de démarrage** et la modification des éléments « machine » demandent les droits admin
  (bouton « Relancer en admin » dans la page Démarrage, à utiliser avec la version compilée).
- La page **Réseau** contacte Cloudflare pour l'adresse publique (à l'ouverture de la page) et pour le test
  de débit (à la demande). Modifier le fichier **hosts** demande les droits admin ; le contenu d'avant la
  dernière modification est gardé dans `hosts-backup.txt`, à côté des réglages.
- La page **SSH** ne lit jamais le contenu des clés privées (seulement leur première ligne, pour les reconnaître).
  Le fichier `config` d'avant la dernière modification est gardé dans `ssh-config-backup.txt`, à côté des réglages.
  Une phrase secrète se choisit dans un terminal, jamais dans Kiosky. Les tunnels se ferment avec Kiosky.
- Le nettoyage ignore les fichiers en cours d'utilisation, et ne touche aux fichiers temporaires
  qu'au-delà de 24 h. Ferme les navigateurs pour vider tout leur cache.
- L'expanseur n'enregistre aucune frappe : seuls les 64 derniers caractères restent en mémoire pour la détection.
- Le dossier est dans OneDrive : pense à exclure `node_modules` et `src-tauri/target` de la synchronisation
  (ou déplace le projet hors de OneDrive), sinon OneDrive va synchroniser des Go de fichiers de build.
