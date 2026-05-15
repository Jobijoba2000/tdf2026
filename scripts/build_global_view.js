const fs = require('fs');
const path = require('path');
const earcut = require('earcut');

const FRA_GEOJSON = path.join(__dirname, '../data/geojson/gadm41_FRA_0.geojson');
const GPX_FILE = path.join(__dirname, '../data/gpx/tour-de-france-2026.gpx');
const OUT_FILE = path.join(__dirname, '../data/vue_globale.bin');

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

let fillVertices = []; 
let fillIndices = [];
let fillVertexOffset = 0;

let lineVertices = []; 
let lineIndices = [];
let lineVertexOffset = 0;

function addFillPolygon(polygon) {
    let data = [];
    let holeIndices = [];
    let currentOffset = 0;
    
    polygon.forEach((ring, i) => {
        if (i > 0) holeIndices.push(currentOffset);
        ring.forEach(coord => {
            const p = project(coord[1], coord[0]);
            data.push(p[0], p[1]);
            currentOffset++;
        });
    });
    
    const triangles = (typeof earcut === 'function' ? earcut : earcut.default)(data, holeIndices, 2);
    const base = fillVertexOffset;
    for (let i = 0; i < data.length; i += 2) {
        fillVertices.push(data[i], data[i+1]);
    }
    for (let i = 0; i < triangles.length; i++) {
        fillIndices.push(base + triangles[i]);
    }
    fillVertexOffset += (data.length / 2);
}

function addLineStrip(points, color) {
    if (points.length < 2) return;
    
    let n = points.length;
    let baseOffset = lineVertexOffset;
    
    for (let j = 0; j < n; j++) {
        const p = points[j];
        const pr = j > 0 ? points[j - 1] : p;
        const nx = j < n - 1 ? points[j + 1] : p;
        
        // Vertices: pos.x, pos.y, prev.x, prev.y, next.x, next.y, side, color
        lineVertices.push(p[0], p[1], pr[0], pr[1], nx[0], nx[1], 1.0, color);
        lineVertices.push(p[0], p[1], pr[0], pr[1], nx[0], nx[1], -1.0, color);
        
        if (j < n - 1) {
            let b = baseOffset + j * 2;
            lineIndices.push(b, b + 1, b + 2, b + 1, b + 3, b + 2);
        }
    }
    lineVertexOffset += n * 2;
}

// 1. France
console.log("Parsing France GeoJSON...");
const rawGeo = fs.readFileSync(FRA_GEOJSON, 'utf8');
const parsed = JSON.parse(rawGeo);

parsed.features.forEach(feature => {
    const geometry = feature.geometry;
    const polygons = geometry.type === 'MultiPolygon' ? geometry.coordinates : [geometry.coordinates];
    
    polygons.forEach(polygon => {
        // Remplissage
        addFillPolygon(polygon);
        
        // Contours (STROKES)
        polygon.forEach(ring => {
            let pts = ring.map(coord => project(coord[1], coord[0]));
            addLineStrip(pts, 0.5); // Gris clair pour les côtes
        });
    });
});

// 2. GPX Stages
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
    if (pts.length > 0) {
        addLineStrip(pts, 1.0); // Blanc pur pour les étapes
    }
}

console.log("Writing to vue_globale.bin...");
const totalSize = 16 + (fillVertices.length * 4) + (fillIndices.length * 4) + (lineVertices.length * 4) + (lineIndices.length * 4);
const buf = Buffer.alloc(totalSize);

buf.writeUInt32LE(fillVertices.length / 2, 0);
buf.writeUInt32LE(fillIndices.length, 4);
buf.writeUInt32LE(lineVertices.length / 8, 8);
buf.writeUInt32LE(lineIndices.length, 12);

let offset = 16;
for (let v of fillVertices) { buf.writeFloatLE(v, offset); offset += 4; }
for (let i of fillIndices) { buf.writeUInt32LE(i, offset); offset += 4; }
for (let v of lineVertices) { buf.writeFloatLE(v, offset); offset += 4; }
for (let i of lineIndices) { buf.writeUInt32LE(i, offset); offset += 4; }

fs.writeFileSync(OUT_FILE, buf);
console.log("Done! Fill Verts:", fillVertices.length/2, "Lines Verts:", lineVertices.length/8);
