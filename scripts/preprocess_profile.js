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

    const descMatch = trkContent.match(/<desc>([\s\S]*?)<\/desc>/);
    let start = "Unknown", finish = "Unknown", date = "Unknown";
    if (descMatch) {
        const desc = descMatch[1].replace('<![CDATA[', '').replace(']]>', '');
        const lines = desc.split('\n');
        if (lines[0]) {
            const cities = lines[0].split(/\s*>\s*/);
            start = cities[0] || "Unknown";
            finish = cities[1] || "Unknown";
        }
        if (lines[1]) {
            date = lines[1];
        }
    }

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
        stages.push({ name, start, finish, date, points });
    }
}

console.log(`Found ${stages.length} stages.`);

// --- Build all stages ---
const allStagesData = [];
for (const stage of stages) {
    // 1. Calculate center and local projection
    let sumLat = 0, sumLon = 0;
    for (const pt of stage.points) {
        sumLat += pt.lat;
        sumLon += pt.lon;
    }
    const latCenter = sumLat / stage.points.length;
    const lonCenter = sumLon / stage.points.length;

    // Meters per degree approximation
    const rad = Math.PI / 180;
    const m_per_lat = 111320.0;
    const m_per_lon = 111320.0 * Math.cos(latCenter * rad);

    let totalDist = 0;
    let lastCoord = null;
    let profilePoints = [];
    for (const pt of stage.points) {
        if (lastCoord) {
            totalDist += haversineDistance(lastCoord.lat, lastCoord.lon, pt.lat, pt.lon);
        }
        // Local X, Y in meters
        const lx = (pt.lon - lonCenter) * m_per_lon;
        const ly = (pt.lat - latCenter) * m_per_lat;

        profilePoints.push({ dist: totalDist, ele: pt.ele, lx, ly });
        lastCoord = pt;
    }

    const vertices = [];
    const indices = [];
    let r_v_offset = 0;
    const n = profilePoints.length;
    for (let j = 0; j < n; j++) {
        const p = profilePoints[j];
        const pr = j > 0 ? profilePoints[j - 1] : p;
        const nx = j < n - 1 ? profilePoints[j + 1] : p;

        // Structure per vertex: 13 floats
        // [dist, ele, lx, ly] x 3 (current, prev, next) + side
        const pushV = (side) => {
            vertices.push(p.dist, p.ele, p.lx, p.ly);
            vertices.push(pr.dist, pr.ele, pr.lx, pr.ly);
            vertices.push(nx.dist, nx.ele, nx.lx, nx.ly);
            vertices.push(side);
        };

        pushV(1.0);
        pushV(-1.0);

        if (j < n - 1) {
            const b = r_v_offset;
            indices.push(b, b + 1, b + 2, b + 1, b + 3, b + 2);
        }
        r_v_offset += 2;
    }

    // Find max elevation for THIS stage
    const maxEle = profilePoints.reduce((max, p) => Math.max(max, p.ele), 0);
    const minEle = profilePoints.reduce((min, p) => Math.min(min, p.ele), 9999);

    // Sparkline (60 points)
    const sparkline = new Float32Array(60);
    for (let i = 0; i < 60; i++) {
        const targetDist = (i / 59) * totalDist;
        // Simple linear search (could be optimized)
        let p = profilePoints[0];
        for (let j = 0; j < profilePoints.length; j++) {
            if (profilePoints[j].dist >= targetDist) {
                p = profilePoints[j];
                break;
            }
        }
        sparkline[i] = p.ele;
    }

    allStagesData.push({
        name: stage.name,
        start: stage.start,
        finish: stage.finish,
        date: stage.date,
        totalDist,
        maxEle,
        minEle,
        sparkline,
        vertices: new Float32Array(vertices),
        indices: new Uint32Array(indices)
    });
}

// --- Binary Format ---
// u32: num_stages
// For each stage:
//   u32: name_len, bytes: name
//   u32: start_len, bytes: start
//   u32: finish_len, bytes: finish
//   u32: date_len, bytes: date
//   f32: totalDist, f32: maxEle, f32: minEle
//   f32[60]: sparkline
//   u32: v_count, u32: i_count
//   float32[]: vertices
//   uint32[]: indices

let totalSize = 4; // num_stages
for (const s of allStagesData) {
    totalSize += 4 + Buffer.from(s.name, 'utf8').length;
    totalSize += 4 + Buffer.from(s.start, 'utf8').length;
    totalSize += 4 + Buffer.from(s.finish, 'utf8').length;
    totalSize += 4 + Buffer.from(s.date, 'utf8').length;
    totalSize += 4 + 4 + 4 + (60 * 4); // dist, maxEle, minEle, sparkline
    totalSize += 4 + 4 + s.vertices.byteLength + s.indices.byteLength;
}

const finalBuf = Buffer.alloc(totalSize);
let offset = 0;

finalBuf.writeUInt32LE(allStagesData.length, offset); offset += 4;

for (const s of allStagesData) {
    const writeStr = (str) => {
        const b = Buffer.from(str, 'utf8');
        finalBuf.writeUInt32LE(b.length, offset); offset += 4;
        b.copy(finalBuf, offset); offset += b.length;
    };
    writeStr(s.name);
    writeStr(s.start);
    writeStr(s.finish);
    writeStr(s.date);

    finalBuf.writeFloatLE(s.totalDist, offset); offset += 4;
    finalBuf.writeFloatLE(s.maxEle, offset); offset += 4;
    finalBuf.writeFloatLE(s.minEle, offset); offset += 4;

    for (let i = 0; i < 60; i++) {
        finalBuf.writeFloatLE(s.sparkline[i], offset); offset += 4;
    }

    finalBuf.writeUInt32LE(s.vertices.length, offset); offset += 4;
    finalBuf.writeUInt32LE(s.indices.length, offset); offset += 4;

    Buffer.from(s.vertices.buffer).copy(finalBuf, offset); offset += s.vertices.byteLength;
    Buffer.from(s.indices.buffer).copy(finalBuf, offset); offset += s.indices.byteLength;
}

const zlib = require('zlib');
const compressedBuf = zlib.gzipSync(finalBuf);
fs.writeFileSync(binPath, compressedBuf);
console.log(`Saved ${allStagesData.length} stages to ${binPath} (Original: ${(totalSize / 1024 / 1024).toFixed(2)} MB, Compressed: ${(compressedBuf.length / 1024 / 1024).toFixed(2)} MB)`);
