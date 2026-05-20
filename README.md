# 🚴‍♂️ Cycling Visualizer — Visualiseur de Profils 2D/3D Interactif

Une application de bureau ultra-performante développée en **Rust**, permettant de visualiser de manière interactive les étapes du **Tour de France 2026** ainsi que d'autres courses majeures (**Giro d'Italia 2026** et **Vuelta a España 2026**). Propulsée par **wgpu** (WebGPU pour Rust) et **glam**, elle offre un rendu graphique 3D en temps réel à 60 FPS avec des transitions animées fluides (morphing) et une interface utilisateur haut de gamme.

https://github.com/user-attachments/assets/6d848c80-9903-4a48-ad48-35eae3c320f3

---

## 🌟 Fonctionnalités Principales

* **Visualisation Multi-Courses** : Naviguez et changez instantanément de course entre le Tour de France, le Giro d'Italia et la Vuelta a España via un menu d'accueil dynamique (touche **M**).
* **Profils 2D Détaillés** : Affichage précis de la courbe d'altitude de chaque étape en fonction de sa distance.
* **Calculateur de Pente Interactif** : Maintenez la touche **Ctrl** et faites un **clic gauche** sur le profil pour définir un point de départ, puis un second Ctrl+clic pour le point d'arrivée : l'application trace instantanément une zone rouge de sélection et calcule le pourcentage moyen de la pente, la distance et le dénivelé. Pour annuler ou quitter ce mode à tout moment, effectuez un simple **clic droit** sur le profil.
* **Morphing 2D ➡️ 3D** : Transition animée fluide passant d'une courbe de profil 2D à un tracé 3D extrudé et surélevé dans l'espace.
* **Caméra 3D Libre** : Rotation, inclinaison et zoom ultra-fluides avec gestion de l'inertie physique pour une navigation naturelle.
* **Carte Globale interactive** : Basculez sur la vue "France/Italie/Espagne" pour afficher la carte entière du pays et le tracé géographique exact de toutes les étapes de la course sélectionnée.
* **Thèmes & Couleurs Personnalisables** : Basculez d'une seule touche entre les couleurs officielles de chaque course (Jaune TDF, Rose Giro, Rouge Vuelta) et une couleur alternative vert néon très contrastée.
* **Dashboard Premium & Barre Latérale** : Une colonne latérale élégante affichant les cartes de chaque étape avec leur date, distance et un profil simplifié (sparkline) dont le remplissage et les contours s'harmonisent parfaitement avec le graphique principal.
* **Typographie Haute Clarté** : Intégration de caractères accentués multilingues (comme le "ñ" de España) et d'un lissage de texte avec effet de gras et contour noir pour une lisibilité parfaite dans toutes les résolutions.

---

## 🛠️ Compilation et Lancement

### Prérequis
Vous devez disposer de la chaîne de compilation **Rust** installée sur votre machine. Si ce n'est pas le cas, installez Rust via [rustup.rs](https://rustup.rs/).

### Lancement en mode Développement
Pour compiler et lancer rapidement l'application en cours de développement :
```bash
cargo run
```

### Lancement en mode Release (Optimisé)
Pour bénéficier d'une fluidité maximale à 60 FPS constants, lancez l'application avec les optimisations maximales du compilateur :
```bash
cargo run --release
```

---

## 📦 Générer l'Exécutable et Distribuer l'Application (Windows)

Bien que la police de caractères (`font.ttf`) soit directement intégrée à l'exécutable lors de la compilation pour simplifier le rendu, les données géographiques et de profils des courses (`profile.bin` et `global.bin`) sont lues dynamiquement depuis le disque au démarrage de l'application.

Pour compiler le projet :
```bash
cargo build --release
```

Une fois la compilation terminée, vous obtiendrez le fichier exécutable à cet emplacement :
```text
target/release/cycling-visualizer.exe
```

**Distribution de l'application :**
Pour faire fonctionner l'application sur un autre ordinateur Windows, vous devez distribuer l'exécutable `cycling-visualizer.exe` **accompagné de son dossier de données** `data/races/`. 

Un script d'exportation automatisé est fourni à la racine. Pour compiler l'application en mode Release et packager automatiquement l'exécutable avec son dossier de données, il vous suffit de lancer la commande suivante dans votre terminal Windows :
```cmd
export
```
*(ou double-cliquez sur `export.bat` à la racine)*. Cela va générer un dossier d'exportation prêt à être distribué sous `exports/cycling-visualizer/`.

L'arborescence minimale de distribution finale doit ressembler à ceci :
```text
├── cycling-visualizer.exe
└── data/
    └── races/
        ├── tdf/
        │   ├── meta.json
        │   ├── profile.bin
        │   └── global.bin
        ├── giro/
        └── vuelta/
```

---

## 🖥️ Instructions Multiplateforme (Linux & macOS)

L'application est conçue pour être multiplateforme et fonctionne nativement sur tous les systèmes d'exploitation majeurs.

### 🍎 macOS
L'application fonctionne de manière native sur macOS (Intel et Apple Silicon) en exploitant l'API **Metal** d'Apple.
1. Ouvrez votre terminal dans le dossier du projet.
2. Compilez et lancez avec :
   ```bash
   cargo run --release
   ```
Aucune dépendance externe n'est requise.

### 🐧 Linux
Sur Linux, l'application utilise l'API **Vulkan** ou **OpenGL**. Pour pouvoir compiler et faire fonctionner l'interface graphique via `wgpu` et `winit`, vous devez installer les bibliothèques de développement graphiques système.

Sur les distributions basées sur **Debian / Ubuntu / Mint**, exécutez la commande suivante dans votre terminal avant de lancer la compilation :
```bash
sudo apt update
sudo apt install -y pkg-config libx11-dev libxi-dev libxrandr-dev libudev-dev libwayland-dev libxkbcommon-dev
```

Une fois ces paquets installés, vous pouvez compiler et lancer le projet normalement :
```bash
cargo run --release
```

---

## 🎮 Contrôles et Raccourcis

### Navigation & Sélection
* **Sélectionner une étape** : Cliquez sur une carte d'étape dans la colonne de gauche, ou cliquez directement sur le tracé rouge d'une étape sur la carte globale.
* **Défiler la liste des étapes** : Utilisez la **molette de la souris** au-dessus de la colonne latérale de gauche.
* **Menu Principal / Changer de Course** : Appuyez sur la touche **M**.

### Graphique Principal (Detailed View)
* **Clic gauche + Glisser** :
  * En mode **2D** : Déplacez latéralement (panoramique) le profil.
  * En mode **3D** / **Global** : Faites tourner la caméra dans l'espace autour du profil ou de la carte géographique.
* **Molette de la souris** : Zoom avant / arrière au niveau du pointeur de la souris (avec un alignement vertical stable du profil 2D).
* **Ctrl + Clic gauche sur le profil 2D** :
  * Premier clic : Définit le point de départ de la mesure de pente (une ligne verticale blanche limitée au profil s'affiche).
  * Deuxième clic : Définit le point d'arrivée. Affiche une zone de sélection rouge vif, deux délimiteurs verticaux blancs arrêtés pile au profil, et le résultat calculé de la pente sous forme de texte double-ligne centré (pourcentage en haut, dénivelé et distance en bas).
* **Clic droit sur le profil 2D** : Sort instantanément du mode de calcul de pente (réinitialise l'état, effaçant le tracé rouge, les lignes et les textes).

### Interface & Boutons
* **Changer de Couleur (Theme)** : Appuyez sur la touche **C** pour alterner à tout moment entre les couleurs officielles de la course (Jaune, Rose, Rouge) et le Vert Néon à fort contraste.
* **Bouton "3D"** (ou touche **Espace**) : Déclenche la transition morphing fluide entre le profil 2D et le modèle 3D extrudé de l'étape.
* **Bouton "Global"** (ou touche **Entrée**) : Active ou désactive la vue d'ensemble de la carte géographique.
