const fs = require('fs');
const path = require('path');

const FRA_GEOJSON = path.join(__dirname, '../data/geojson/gadm41_FRA_0.geojson');
const GPX_FILE = path.join(__dirname, '../data/gpx/tour-de-france-2026.gpx');
const OUT_FILE = path.join(__dirname, '../data/vue_globale.bin');

// Fixed Global Projection (Center of France)
const GLOBAL_LAT = 46.5;
const GLOBAL_LON = 2.5;
const rad = Math.PI / 180;
const m_per_lat = 111320.0;
const m_per_lon = 111320.0 * Math.cos(GLOBAL_LAT * rad);

function project(lat, lon) {
    const lx = (lon - GLOBAL_LON) * m_per_lon;
    const ly = (lat - GLOBAL_LAT) * m_per_lat;
    return [lx, ly];
}

let allVertices = [];
let allIndices = [];
let vertexOffset = 0;

function addLineStrip(points, color) {
    if (points.length < 2) return;
    
    let n = points.length;
    for (let j = 0; j < n; j++) {
        const p = points[j];
        const pr = j > 0 ? points[j - 1] : p;
        const nx = j < n - 1 ? points[j + 1] : p;
        
        // 8 floats: pos.x, pos.y, prev.x, prev.y, next.x, next.y, side, color
        const pushV = (side) => {
            allVertices.push(p[0], p[1]);
            allVertices.push(pr[0], pr[1]);
            allVertices.push(nx[0], nx[1]);
            allVertices.push(side);
            allVertices.push(color);
        };
        
        pushV(1.0);
        pushV(-1.0);
        
        if (j < n - 1) {
            const b = vertexOffset;
            allIndices.push(b, b + 1, b + 2, b + 1, b + 3, b + 2);
        }
        vertexOffset += 2;
    }
}

// 1. Parse France boundaries
console.log("Parsing France GeoJSON...");
const rawGeo = fs.readFileSync(FRA_GEOJSON, 'utf8');
const parsed = JSON.parse(rawGeo);

parsed.features.forEach(feature => {
    if (feature.geometry.type === 'MultiPolygon') {
        feature.geometry.coordinates.forEach(polygon => {
            polygon.forEach(ring => {
                let pts = ring.map(coord => project(coord[1], coord[0]));
                addLineStrip(pts, 0.4); // Gris moyen pour la carte de france
            });
        });
    } else if (feature.geometry.type === 'Polygon') {
        feature.geometry.coordinates.forEach(ring => {
            let pts = ring.map(coord => project(coord[1], coord[0]));
            addLineStrip(pts, 0.4);
        });
    }
});

// 2. Parse GPX
console.log("Parsing GPX stages...");
const gpxData = fs.readFileSync(GPX_FILE, 'utf8');

const trkRegex = /<trk>[\s\S]*?<\/trk>/g;
let match;
while ((match = trkRegex.exec(gpxData)) !== null) {
    let pts = [];
    const trkContent = match[0];
    const ptRegex = /<trkpt\s+lat="([^"]+)"\s+lon="([^"]+)">/g;
    let ptMatch;
    while ((ptMatch = ptRegex.exec(trkContent)) !== null) {
        pts.push(project(parseFloat(ptMatch[1]), parseFloat(ptMatch[2])));
    }
    // Blanc franc pour les étapes
    addLineStrip(pts, 1.0);
}

// Write to bin
console.log("Writing to vue_globale.bin...");
const buf = Buffer.alloc(8 + allVertices.length * 4 + allIndices.length * 4);
buf.writeUInt32LE(allVertices.length / 8, 0); // Nombre de sommets complets
buf.writeUInt32LE(allIndices.length, 4);

let offset = 8;
for (let v of allVertices) {
    buf.writeFloatLE(v, offset);
    offset += 4;
}
for (let i of allIndices) {
    buf.writeUInt32LE(i, offset);
    offset += 4;
}

fs.writeFileSync(OUT_FILE, buf);
console.log("Done! Vertices:", allVertices.length / 8, "Indices:", allIndices.length);
