# 🚴‍♂️ Tour de France 2026 — Visualiseur de Profils 2D/3D Interactif

Une application de bureau ultra-performante développée en **Rust**, permettant de visualiser de manière interactive les étapes du **Tour de France 2026**. Propulsée par **wgpu** (WebGPU pour Rust) et **glam**, elle offre un rendu graphique 3D en temps réel à 60 FPS avec des transitions animées fluides (morphing) et une interface utilisateur haut de gamme.

---

## 🌟 Fonctionnalités Principales

* **Profils 2D Détaillés** : Affichage précis de la courbe d'altitude de chaque étape en fonction de sa distance.
* **Calculateur de Pente Interactif** : Maintenez la touche **Ctrl** et faites un **clic gauche** sur le profil pour définir un point de départ, puis un autre Ctrl+clic pour le point d'arrivée : l'application calcule instantanément la distance, la différence d'altitude (dénivelé) et le pourcentage de la pente moyenne.
* **Morphing 2D ➡️ 3D** : Transition animée fluide passant d'une courbe de profil 2D à un tracé 3D extrudé et surélevé dans l'espace.
* **Caméra 3D Libre** : Rotation, inclinaison et zoom ultra-fluides avec gestion de l'inertie physique pour une navigation naturelle.
* **Carte Globale interactive** : Basculez sur la vue "France" pour afficher la carte entière du pays et le tracé géographique exact de toutes les étapes du Tour.
* **Dashboard Premium & Barre Latérale** : Une colonne latérale élégante affichant les cartes de chaque étape avec leur date, distance et un profil simplifié (sparkline) dont le remplissage et les contours s'harmonisent parfaitement avec le graphique principal.

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

## 📦 Générer un Exécutable `.exe` Autonome (Windows)

L'une des grandes forces de cette application est sa portabilité absolue. Tous les fichiers de ressources (données géographiques des étapes, tracés 3D, courbes simplifiées, et polices d'écriture) sont compressés et intégrés **directement à l'intérieur du binaire** lors de la compilation grâce au mécanisme `include_bytes!` de Rust.

Pour générer l'exécutable autonome :
```bash
cargo build --release
```

Une fois la compilation terminée, vous obtiendrez le fichier exécutable à cet emplacement :
```text
target/release/tdf2026.exe
```

**Note importante :** Ce fichier `.exe` est **100% autonome**. Vous pouvez le copier, le renommer, et l'envoyer sur n'importe quel ordinateur Windows sans avoir besoin de copier le dossier `data/` ou tout autre fichier externe. L'application démarrera instantanément avec toutes ses ressources intégrées !

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
sudo apt install -y pkg-config libx11-dev libxi-dev libxrandr-dev libudev-dev libwayland-dev
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

### Graphique Principal (Detailed View)
* **Clic gauche + Glisser** :
  * En mode **2D** : Déplacez latéralement (panoramique) le profil.
  * En mode **3D** / **Global** : Faites tourner la caméra dans l'espace autour du profil ou de la carte de France.
* **Molette de la souris** : Zoom avant / arrière au niveau du pointeur de la souris.
* **Ctrl + Clic gauche sur le profil 2D** :
  * Premier clic : Définit le point de départ de la mesure de pente (une ligne verticale jaune s'affiche).
  * Deuxième clic : Définit le point d'arrivée. Affiche un encadré flottant avec la distance parcourue, le dénivelé et le pourcentage moyen de la pente.
  * Troisième clic : Réinitialise l'outil de mesure de pente.

### Interface & Boutons
* **Bouton "3D"** (ou touche **Espace**) : Déclenche la transition morphing fluide entre le profil 2D et le modèle 3D extrudé de l'étape.
* **Bouton "Global"** (ou touche **Entrée**) : Active ou désactive la vue d'ensemble de la carte de France.
