const fs = require('fs');
const path = require('path');

// Basic XML parsing via regex since we just need simple tags
// to avoid heavy dependencies, we can just extract data via regex for the GPX.
// A GPX file is highly structured.
const gpxPath = path.join(__dirname, '../data/gpx/tour-de-france-2026.gpx');
const binPath = path.join(__dirname, '../data/profile.bin');

if (!fs.existsSync(gpxPath)) {
    console.error("GPX file not found:", gpxPath);
    process.exit(1);
}

const gpxData = fs.readFileSync(gpxPath, 'utf-8');

function haversineDistance(lat1, lon1, lat2, lon2) {
    const R = 6371000.0; // metres
    const rad = Math.PI / 180;
    const phi1 = lat1 * rad;
    const phi2 = lat2 * rad;
    const deltaPhi = (lat2 - lat1) * rad;
    const deltaLambda = (lon2 - lon1) * rad;

    const a = Math.sin(deltaPhi / 2) * Math.sin(deltaPhi / 2) +
              Math.cos(phi1) * Math.cos(phi2) *
              Math.sin(deltaLambda / 2) * Math.sin(deltaLambda / 2);
    const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));

    return R * c;
}

console.log("Parsing GPX and extracting stages...");

const trkRegex = /<trk>[\s\S]*?<\/trk>/g;
let match;
let maxGlobalEle = 0;
let stages = [];

while ((match = trkRegex.exec(gpxData)) !== null) {
    const trkContent = match[0];
    
    const nameMatch = trkContent.match(/<name>(.*?)<\/name>/);
    const name = nameMatch ? nameMatch[1].replace('<![CDATA[', '').replace(']]>', '') : "Unknown";
    
    const ptRegex = /<trkpt\s+lat="([^"]+)"\s+lon="([^"]+)">[\s\S]*?<ele>([^<]+)<\/ele>[\s\S]*?<\/trkpt>/g;
    let ptMatch;
    
    let points = [];
    while ((ptMatch = ptRegex.exec(trkContent)) !== null) {
        const lat = parseFloat(ptMatch[1]);
        const lon = parseFloat(ptMatch[2]);
        const ele = parseFloat(ptMatch[3]);
        
        if (ele > maxGlobalEle) {
            maxGlobalEle = ele;
        }
        
        points.push({ lat, lon, ele });
    }
    
    if (points.length > 0) {
        stages.push({ name, points });
    }
}

console.log(`Found ${stages.length} stages. Global Max Elevation: ${maxGlobalEle}m`);

// Choose a mountain stage (e.g. Stage 12, or the one with highest elevation)
let selectedStage = stages.find(s => s.name.includes("Etape 12") || s.name.includes("Etape 11"));
if (!selectedStage) {
    // Fallback to the stage with the most points
    selectedStage = stages.reduce((prev, current) => (prev.points.length > current.points.length) ? prev : current);
}

console.log(`Processing selected stage: ${selectedStage.name} (${selectedStage.points.length} points)`);

// Calculate distances
let totalDist = 0;
let lastCoord = null;
let profilePoints = []; // { dist, ele }

for (const pt of selectedStage.points) {
    if (lastCoord) {
        totalDist += haversineDistance(lastCoord.lat, lastCoord.lon, pt.lat, pt.lon);
    }
    profilePoints.push({ dist: totalDist, ele: pt.ele });
    lastCoord = pt;
}

// Generate vertices for thick line (just like atlas-native lines)
// Format: [x, y, prev_x, prev_y, next_x, next_y, side]
// We generate 2 vertices per point (side 1 and -1)
const vertices = [];
const indices = [];

let r_v_offset = 0;
const n = profilePoints.length;

for (let j = 0; j < n; j++) {
    const p = profilePoints[j];
    const pr = j > 0 ? profilePoints[j - 1] : p;
    const nx = j < n - 1 ? profilePoints[j + 1] : p;

    // Side 1
    vertices.push(p.dist, p.ele, pr.dist, pr.ele, nx.dist, nx.ele, 1.0);
    // Side -1
    vertices.push(p.dist, p.ele, pr.dist, pr.ele, nx.dist, nx.ele, -1.0);

    if (j < n - 1) {
        const b = r_v_offset;
        indices.push(b, b + 1, b + 2, b + 1, b + 3, b + 2);
    }
    r_v_offset += 2;
}

// Write to binary file
// Header: 
// - max_dist (Float32)
// - max_ele_global (Float32)
// - num_vertices (Uint32)
// - num_indices (Uint32)
// Data:
// - vertices (Float32Array)
// - indices (Uint32Array)

const headerSize = 16;
const verticesSize = vertices.length * 4;
const indicesSize = indices.length * 4;

const buffer = Buffer.alloc(headerSize + verticesSize + indicesSize);
let offset = 0;

buffer.writeFloatLE(totalDist, offset); offset += 4;
buffer.writeFloatLE(maxGlobalEle, offset); offset += 4;
buffer.writeUInt32LE(vertices.length, offset); offset += 4;
buffer.writeUInt32LE(indices.length, offset); offset += 4;

for (const v of vertices) {
    buffer.writeFloatLE(v, offset);
    offset += 4;
}

for (const i of indices) {
    buffer.writeUInt32LE(i, offset);
    offset += 4;
}

fs.writeFileSync(binPath, buffer);
console.log(`Saved profile to ${binPath}`);
console.log(`Vertices: ${vertices.length}, Indices: ${indices.length}`);
