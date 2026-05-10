# Définition des balises du fichier GPX (Tour de France 2026)

Un fichier GPX (GPS Exchange Format) est un format de données XML permettant d'échanger des coordonnées GPS. Voici la signification des différentes balises présentes dans votre fichier `tour-de-france-2026.gpx` :

## Balises principales
- `<gpx>` : C'est la balise racine (principale) qui englobe tout le contenu du fichier. Elle définit la version du format GPX utilisé et le créateur du fichier.
- `<metadata>` : Contient les métadonnées (informations générales) du fichier GPX.
- `<name>` : Le nom général du fichier, du point d'intérêt, ou du tracé (par exemple : "Tour de France 2026" ou "Etape 1").
- `<desc>` : Une description textuelle associée au fichier, à l'étape ou au point d'intérêt.

## Points d'intérêt (Waypoints)
- `<wpt>` (Waypoint) : Représente un point d'intérêt isolé sur la carte. Il possède des attributs `lat` (latitude) et `lon` (longitude) pour définir ses coordonnées exactes. Dans le contexte du Tour de France, cela peut représenter le point de départ de l'étape, un col, un sprint intermédiaire, etc.

## Tracés (Tracks)
- `<trk>` (Track) : Représente un parcours complet ou un trajet (par exemple, une étape entière du Tour de France). Un `<trk>` peut contenir plusieurs segments.
- `<trkseg>` (Track Segment) : Un segment de tracé. C'est une série continue de points de passage. Si le signal GPS est perdu ou s'il y a une interruption dans le trajet, un nouveau `<trkseg>` est généralement créé.
- `<trkpt>` (Track Point) : Un point de passage individuel qui compose le tracé. Comme le `<wpt>`, il possède des coordonnées `lat` et `lon`. Mis bout à bout, ces points forment le tracé de l'étape sur la carte.
- `<ele>` (Elevation) : L'altitude ou l'élévation du point (que ce soit un `<wpt>` ou un `<trkpt>`), exprimée en mètres.

## Extensions (Personnalisation visuelle)
- `<extensions>` : Cette balise permet d'ajouter des informations spécifiques qui ne font pas partie du standard GPX de base.
- `<line>` : Une balise d'extension souvent utilisée par des outils de visualisation (comme VisuGPX) pour définir comment dessiner le tracé sur la carte.
- `<color>` : La couleur du tracé (au format hexadécimal, par exemple `ff0000` pour le rouge).
- `<opacity>` : L'opacité (ou transparence) de la ligne dessinée.
- `<width>` : L'épaisseur de la ligne dessinée sur la carte.
