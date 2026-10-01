# Toolbox

Petite boîte à outils Windows qui vit dans la zone de notification.
Rust + Tauri 2 pour la logique, Svelte 5 pour l'interface (style Windows 11, Mica).

| Module | Ce qu'il fait |
|---|---|
| **Palette** | Raccourci global (`Ctrl+Shift+Space` par défaut) : recherche façon menu Démarrer (applications, projets, dossiers, Paramètres Windows, outils système, verrouiller/veille/redémarrer, pages de Toolbox) qui apprend de tes habitudes, plus calculs, unités, devises, fuseaux, encodages `kill 3000` / `port 3000`, recherche web (`g`, `yt`, `gh`, `mdn`, `npm`, `crates`, `so`, `wiki`…) et commandes `>ipconfig` avec la sortie affichée |
| **Expanseur de texte** | `;mail` → ton adresse, `;sig` → ta signature, `;date` → la date du jour, dans toutes les applications |
| **Pipette** | Raccourci global (`Win+Shift+C` par défaut) : loupe sous la souris, clic = couleur copiée en HEX, RGB ou HSL, historique |
| **Projets** | Détecte les projets (Git, Node, Rust, PHP, .NET, Android…), branche et modifications non commitées, ouverture dans l'éditeur adapté |
| **Dossiers** | Un raccourci global par dossier favori (`Ctrl+Shift+1`…), ouvert dans l'Explorateur, le terminal ou un éditeur |
| **Variables** | PATH et variables utilisateur/système : chemins introuvables et doublons signalés, réordonnancement, annulation de la dernière modification |
| **Conteneurs** | Distributions WSL (terminal, fichiers, démarrer/arrêter, par défaut) et conteneurs Docker groupés par projet compose (démarrer, arrêter, journaux, terminal, ports), lancement de Docker Desktop |
| **Volume** | Volume et sourdine par application, préréglages (Jeu, Réunion, Musique…) |
| **Moniteur** | CPU, mémoire et réseau en direct (2 min d'historique), espace disque, processus les plus gourmands (avec arrêt), résumé dans l'infobulle et jauge CPU comme icône |
| **Démarrage** | Programmes lancés avec Windows (registre, dossier Démarrage, tâches planifiées), activation/désactivation, temps de démarrage |
| **Nettoyage** | Temp, caches des navigateurs, rapports d'erreur, Windows Update, caches npm/pip, corbeille, avec la place gagnée. Recherche de dossiers par nom (`node_modules`, `target`, `.venv`…) dans les dossiers choisis |
| **Ports** | Qui écoute sur quel port (« le port 3000 est-il libre ? »), processus propriétaire, bouton pour l'arrêter. Les ports de Windows peuvent être masqués |

**Barre flottante** (Réglages → Barre flottante) : petite barre toujours visible, posée sur la barre des tâches (à gauche ou à droite) ou déplaçable, avec les éléments choisis : CPU, RAM, réseau, heure, date, batterie, serveurs locaux, conteneurs Docker, état Git du projet favori, musique en cours (lecture/pause, suivant), volume (molette), minuteur Pomodoro. Clic sur un élément : son action ; survol : le détail. Elle se cache quand une application est en plein écran.

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

## Organisation

```
src/                     Interface Svelte
  pages/                 Une page par module
  lib/                   Composants partagés, appels à Rust (api.ts)
  Palette.svelte         Fenêtre flottante du convertisseur
  Picker.svelte          Loupe de la pipette
  Widget.svelte          Barre flottante
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
  widget.rs              Barre flottante : placement sur la barre des tâches, premier plan, plein écran
  launcher.rs            « Ouvrir avec » : Explorateur, terminal, VS Code, JetBrains…
  colorpicker.rs         Pipette : hook souris + capture d'écran (GDI)
  monitor.rs             CPU / RAM / réseau / disques
  cleaner.rs             Nettoyage (ne suit jamais les liens ni les jonctions)
  ports.rs               Tables TCP/UDP (IP Helper) et arrêt de processus
  startup.rs             Registre, dossiers Démarrage, tâches, journal de performances
  settings.rs            Réglages JSON dans %APPDATA%\com.bebou.toolbox
```

## Bon à savoir

- Fermer la fenêtre la **cache** seulement. Pour quitter : clic droit sur l'icône → Quitter.
- Les **temps de démarrage** et la modification des éléments « machine » demandent les droits admin
  (bouton « Relancer en admin » dans la page Démarrage, à utiliser avec la version compilée).
- Le nettoyage ignore les fichiers en cours d'utilisation, et ne touche aux fichiers temporaires
  qu'au-delà de 24 h. Ferme les navigateurs pour vider tout leur cache.
- L'expanseur n'enregistre aucune frappe : seuls les 64 derniers caractères restent en mémoire pour la détection.
- Le dossier est dans OneDrive : pense à exclure `node_modules` et `src-tauri/target` de la synchronisation
  (ou déplace le projet hors de OneDrive), sinon OneDrive va synchroniser des Go de fichiers de build.
