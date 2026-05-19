const fs = require('fs');
const path = require('path');
const earcut = require('earcut');

// --- CLI Args ---
const args = process.argv.slice(2);
const raceArgIdx = args.indexOf('--race');
if (raceArgIdx === -1 || !args[raceArgIdx + 1]) {
    console.error("Usage: node build_global_view.js --race <tdf|giro|...>");
    process.exit(1);
}
const raceId = args[raceArgIdx + 1];

// --- Race configs ---
const RACE_CONFIGS = {
    tdf: {
        geojsonPath: path.join(__dirname, '../data/geojson/gadm41_FRA_0.geojson'),
        gpxMode: 'single',
        gpxPath: path.join(__dirname, '../data/gpx/tour-de-france-2026.gpx'),
        globalLat: 46.5,
        globalLon: 2.5,
    },
    giro: {
        geojsonPath: path.join(__dirname, '../data/geojson/gadm41_ITA_0.json'),
        gpxMode: 'multi',
        gpxDir: path.join(__dirname, '../data/gpx/giro-d-italia-2026'),
        numStages: 21,
        globalLat: 42.5,
        globalLon: 12.5,
    },
};

if (!RACE_CONFIGS[raceId]) {
    console.error(`Unknown race: "${raceId}". Available: ${Object.keys(RACE_CONFIGS).join(', ')}`);
    process.exit(1);
}

const config = RACE_CONFIGS[raceId];
const outDir = path.join(__dirname, `../data/races/${raceId}`);
const OUT_FILE = path.join(outDir, 'global.bin');

if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });

const GLOBAL_LAT = config.globalLat;
const GLOBAL_LON = config.globalLon;
const rad = Math.PI / 180;
const m_per_lat = 111320.0;
const m_per_lon = 111320.0 * Math.cos(GLOBAL_LAT * rad);

function project(lat, lon) {
    return [(lon - GLOBAL_LON) * m_per_lon, (lat - GLOBAL_LAT) * m_per_lat];
}

let fillVertices = [], fillIndices = [], fillVertexOffset = 0;
let lineVertices = [], lineIndices = [], lineVertexOffset = 0;

function addFillPolygon(polygon) {
    let data = [], holeIndices = [], currentOffset = 0;
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
    for (let i = 0; i < data.length; i += 2) fillVertices.push(data[i], data[i + 1]);
    for (let i = 0; i < triangles.length; i++) fillIndices.push(base + triangles[i]);
    fillVertexOffset += data.length / 2;
}

function addLineStrip(points, color) {
    if (points.length < 2) return;
    let n = points.length;
    let baseOffset = lineVertexOffset;
    for (let j = 0; j < n; j++) {
        const p = points[j];
        const pr = j > 0 ? points[j - 1] : p;
        const nx = j < n - 1 ? points[j + 1] : p;
        lineVertices.push(p[0], p[1], pr[0], pr[1], nx[0], nx[1], 1.0, color);
        lineVertices.push(p[0], p[1], pr[0], pr[1], nx[0], nx[1], -1.0, color);
        if (j < n - 1) {
            let b = baseOffset + j * 2;
            lineIndices.push(b, b + 1, b + 2, b + 1, b + 3, b + 2);
        }
    }
    lineVertexOffset += n * 2;
}

// 1. GeoJSON country fill
console.log(`Parsing GeoJSON: ${config.geojsonPath}`);
const rawGeo = fs.readFileSync(config.geojsonPath, 'utf8');
const parsed = JSON.parse(rawGeo);
parsed.features.forEach(feature => {
    const geometry = feature.geometry;
    const polygons = geometry.type === 'MultiPolygon' ? geometry.coordinates : [geometry.coordinates];
    polygons.forEach(polygon => {
        addFillPolygon(polygon);
        polygon.forEach(ring => {
            let pts = ring.map(coord => project(coord[1], coord[0]));
            addLineStrip(pts, 0.5);
        });
    });
});

// 2. GPX Stages
console.log("Parsing GPX stages...");
if (config.gpxMode === 'single') {
    const gpxData = fs.readFileSync(config.gpxPath, 'utf8');
    const trkRegex = /<trk>[\s\S]*?<\/trk>/g;
    let match;
    while ((match = trkRegex.exec(gpxData)) !== null) {
        let pts = [];
        const ptRegex = /<trkpt\s+lat="([^"]+)"\s+lon="([^"]+)">/g;
        let ptMatch;
        while ((ptMatch = ptRegex.exec(match[0])) !== null) {
            pts.push(project(parseFloat(ptMatch[1]), parseFloat(ptMatch[2])));
        }
        if (pts.length > 0) addLineStrip(pts, 1.0);
    }
} else {
    // Multi-file mode
    for (let i = 1; i <= config.numStages; i++) {
        const gpxFile = path.join(config.gpxDir, `etappe-${i}-route.gpx`);
        if (!fs.existsSync(gpxFile)) { console.warn(`  [WARN] Missing: ${gpxFile}`); continue; }
        const gpxData = fs.readFileSync(gpxFile, 'utf8');
        const ptRegex = /<trkpt\s+lat="([^"]+)"\s+lon="([^"]+)">/g;
        let ptMatch, pts = [];
        while ((ptMatch = ptRegex.exec(gpxData)) !== null) {
            pts.push(project(parseFloat(ptMatch[1]), parseFloat(ptMatch[2])));
        }
        if (pts.length > 0) addLineStrip(pts, 1.0);
    }
}

// 3. Write binary
console.log("Writing to", OUT_FILE);
const totalSize = 16
    + (fillVertices.length * 4) + (fillIndices.length * 4)
    + (lineVertices.length * 4) + (lineIndices.length * 4);
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
console.log(`Done! Fill Verts: ${fillVertices.length / 2}, Line Verts: ${lineVertices.length / 8}`);
