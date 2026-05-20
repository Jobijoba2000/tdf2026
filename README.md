# 🚴‍♂️ Cycling Visualizer

Application de visualisation de tracés et de profils altimétriques de courses cyclistes (Tour de France 2026, Giro 2026, Vuelta 2026). Développée en Rust avec l'API graphique wgpu.

https://github.com/user-attachments/assets/6d848c80-9903-4a48-ad48-35eae3c320f3


---

## 🌟 Vues et Transitions

L'application s'articule autour de 4 vues principales :

1. **Menu Principal** (touche **M**) : Sélection de la course active (Tour de France, Giro, Vuelta).
2. **Vue 2D (Profil d'étape)** : Courbe d'altitude en fonction de la distance de l'étape. Permet de calculer des pourcentages de pentes (Ctrl + Clic gauche pour définir début/fin, Clic droit pour réinitialiser).
3. **Vue 3D (Tracé d'étape)** (touche **Espace** depuis la 2D) : Modèle 3D extrudé et orientable de l'étape. Le passage entre la 2D et la 3D s'effectue via une transition animée par morphing.
4. **Vue Globale (Carte)** (touche **Entrée** depuis l'étape) : Affiche la carte complète du pays de la course avec les tracés de toutes les étapes. Appuyez sur **Espace** pour en sortir et revenir au profil de l'étape.



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

### Raccourcis Clavier
* **M** : Retourner au menu principal (sélection de la course).
* **Espace** : Basculer entre la vue 2D et la vue 3D (ou quitter la vue globale).
* **Entrée** : Entrer dans la vue globale (carte du pays).
* **C** : Alterner entre les couleurs officielles de la course et la couleur vert néon.

### Souris
* **Sélection d'étape** : Clic gauche sur une étape dans la colonne de gauche ou sur le tracé rouge de la carte globale.
* **Scroll** : Molette sur la colonne de gauche pour défiler les étapes, ou sur le graphique pour zoomer.
* **Pan / Rotation** : Clic gauche maintenu et glisser.
* **Calcul de pente** : Maintenir **Ctrl** + **Clic gauche** pour placer les points de départ et de fin sur le profil 2D. **Clic droit** pour annuler.

