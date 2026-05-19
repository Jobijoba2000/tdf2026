const fs = require('fs');
const path = require('path');

// --- CLI Args ---
const args = process.argv.slice(2);
const raceArgIdx = args.indexOf('--race');
if (raceArgIdx === -1 || !args[raceArgIdx + 1]) {
    console.error("Usage: node preprocess_profile.js --race <tdf|giro|...>");
    process.exit(1);
}
const raceId = args[raceArgIdx + 1];

// --- Race configs ---
const RACE_CONFIGS = {
    tdf: {
        gpxMode: 'single',
        gpxPath: path.join(__dirname, '../data/gpx/tour-de-france-2026.gpx'),
        globalLat: 46.5,
        globalLon: 2.5,
        // stages info extracted from GPX <name>/<desc> tags
    },
    giro: {
        gpxMode: 'multi',
        gpxDir: path.join(__dirname, '../data/gpx/giro-d-italia-2026'),
        gpxPrefix: 'etappe-',
        globalLat: 42.5,
        globalLon: 12.5,
        stages: [
            { num: 1,  name: 'Étape 1',  start: 'Nesebăr',            finish: 'Burgas',           date: '08/05/2026' },
            { num: 2,  name: 'Étape 2',  start: 'Burgas',             finish: 'Veliko Tarnovo',   date: '09/05/2026' },
            { num: 3,  name: 'Étape 3',  start: 'Plovdiv',            finish: 'Sofia',            date: '10/05/2026' },
            { num: 4,  name: 'Étape 4',  start: 'Catanzaro',          finish: 'Cosenza',          date: '12/05/2026' },
            { num: 5,  name: 'Étape 5',  start: 'Praia a Mare',       finish: 'Potenza',          date: '13/05/2026' },
            { num: 6,  name: 'Étape 6',  start: 'Paestum',            finish: 'Naples',           date: '14/05/2026' },
            { num: 7,  name: 'Étape 7',  start: 'Formia',             finish: 'Blockhaus',        date: '15/05/2026' },
            { num: 8,  name: 'Étape 8',  start: 'Chieti',             finish: 'Fermo',            date: '16/05/2026' },
            { num: 9,  name: 'Étape 9',  start: 'Cervia',             finish: 'Corno alle Scale', date: '17/05/2026' },
            { num: 10, name: 'Étape 10', start: 'Viareggio',          finish: 'Massa',            date: '19/05/2026' },
            { num: 11, name: 'Étape 11', start: 'Porcari',            finish: 'Chiavari',         date: '20/05/2026' },
            { num: 12, name: 'Étape 12', start: 'Imperia',            finish: 'Novi Ligure',      date: '21/05/2026' },
            { num: 13, name: 'Étape 13', start: 'Alessandria',        finish: 'Verbania',         date: '22/05/2026' },
            { num: 14, name: 'Étape 14', start: 'Aosta',              finish: 'Pila',             date: '23/05/2026' },
            { num: 15, name: 'Étape 15', start: 'Voghera',            finish: 'Milan',            date: '24/05/2026' },
            { num: 16, name: 'Étape 16', start: 'Bellinzona',         finish: 'Carì',             date: '26/05/2026' },
            { num: 17, name: 'Étape 17', start: "Cassano d'Adda",     finish: 'Andalo',           date: '27/05/2026' },
            { num: 18, name: 'Étape 18', start: 'Fai della Paganella',finish: 'Pieve di Soligo',  date: '28/05/2026' },
            { num: 19, name: 'Étape 19', start: 'Feltre',             finish: 'Piani di Pezzè',   date: '29/05/2026' },
            { num: 20, name: 'Étape 20', start: 'Gemona del Friuli',  finish: 'Piancavallo',      date: '30/05/2026' },
            { num: 21, name: 'Étape 21', start: 'Rome',               finish: 'Rome',             date: '31/05/2026' },
        ],
    },
    vuelta: {
        gpxMode: 'multi',
        gpxDir: path.join(__dirname, '../data/gpx/vuelta-a-espana-2026'),
        gpxPrefix: 'stage-',
        globalLat: 40.0,
        globalLon: -3.5,
        stages: [
            { num: 1,  name: 'Étape 1',  start: 'Monaco',            finish: 'Monaco',            date: '22/08/2026' },
            { num: 2,  name: 'Étape 2',  start: 'Monaco',            finish: 'Manosque',          date: '23/08/2026' },
            { num: 3,  name: 'Étape 3',  start: 'Gruisan',           finish: 'Font Romeu',        date: '24/08/2026' },
            { num: 4,  name: 'Étape 4',  start: 'Andorra La Vella',  finish: 'Andorra La Vella',  date: '25/08/2026' },
            { num: 5,  name: 'Étape 5',  start: 'Falset',            finish: 'Roquetes',          date: '26/08/2026' },
            { num: 6,  name: 'Étape 6',  start: 'Alcossebre',        finish: 'Castellón',         date: '27/08/2026' },
            { num: 7,  name: 'Étape 7',  start: "Vall d'Alba",       finish: 'Valdelinares',      date: '28/08/2026' },
            { num: 8,  name: 'Étape 8',  start: 'Puçol',             finish: 'Xeraco',            date: '29/08/2026' },
            { num: 9,  name: 'Étape 9',  start: 'La Villa Joiosa',   finish: 'Alto de Aitana',    date: '30/08/2026' },
            { num: 10, name: 'Étape 10', start: 'Alcaraz',           finish: 'Elche de la Sierra',date: '01/09/2026' },
            { num: 11, name: 'Étape 11', start: 'Cartagena',         finish: 'Lorca',             date: '02/09/2026' },
            { num: 12, name: 'Étape 12', start: 'Vera',              finish: 'Calar Alto',        date: '03/09/2026' },
            { num: 13, name: 'Étape 13', start: 'Almuñécar',         finish: 'Loja',              date: '04/09/2026' },
            { num: 14, name: 'Étape 14', start: 'Jaén',              finish: 'Sierra de la Pandera',date: '05/09/2026' },
            { num: 15, name: 'Étape 15', start: 'Palma del Río',     finish: 'Córdoba',           date: '06/09/2026' },
            { num: 16, name: 'Étape 16', start: 'Cortegana',         finish: 'La Rábida',         date: '08/09/2026' },
            { num: 17, name: 'Étape 17', start: 'Dos Hermanas',      finish: 'Sevilla',           date: '09/09/2026' },
            { num: 18, name: 'Étape 18', start: 'El Puerto de Santa Maria', finish: 'Jerez de la Frontera', date: '10/09/2026' },
            { num: 19, name: 'Étape 19', start: 'Vélez-Málaga',      finish: 'Peñas Blancas',     date: '11/09/2026' },
            { num: 20, name: 'Étape 20', start: 'La Calahorra',      finish: 'Collada de Alguacil',date: '12/09/2026' },
            { num: 21, name: 'Étape 21', start: 'Granada',           finish: 'Granada',           date: '13/09/2026' },
        ],
    },
};

if (!RACE_CONFIGS[raceId]) {
    console.error(`Unknown race: "${raceId}". Available: ${Object.keys(RACE_CONFIGS).join(', ')}`);
    process.exit(1);
}

const config = RACE_CONFIGS[raceId];
const outDir = path.join(__dirname, `../data/races/${raceId}`);
const binPath = path.join(outDir, 'profile.bin');

if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });

// --- Haversine ---
function haversineDistance(lat1, lon1, lat2, lon2) {
    const R = 6371000.0;
    const rad = Math.PI / 180;
    const phi1 = lat1 * rad, phi2 = lat2 * rad;
    const deltaPhi = (lat2 - lat1) * rad;
    const deltaLambda = (lon2 - lon1) * rad;
    const a = Math.sin(deltaPhi / 2) ** 2 +
        Math.cos(phi1) * Math.cos(phi2) * Math.sin(deltaLambda / 2) ** 2;
    return R * 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
}

// --- Parse points from GPX content ---
function parsePoints(gpxContent) {
    const ptRegex = /<trkpt\s+lat="([^"]+)"\s+lon="([^"]+)">[\s\S]*?<ele>([^<]+)<\/ele>[\s\S]*?<\/trkpt>/g;
    let ptMatch;
    const points = [];
    while ((ptMatch = ptRegex.exec(gpxContent)) !== null) {
        const lat = parseFloat(ptMatch[1]);
        const lon = parseFloat(ptMatch[2]);
        const ele = parseFloat(ptMatch[3]);
        if (!isNaN(lat) && !isNaN(lon) && !isNaN(ele)) {
            points.push({ lat, lon, ele });
        }
    }
    return points;
}

// --- Extract stages ---
let stages = [];

if (config.gpxMode === 'single') {
    if (!fs.existsSync(config.gpxPath)) {
        console.error("GPX file not found:", config.gpxPath);
        process.exit(1);
    }
    const gpxData = fs.readFileSync(config.gpxPath, 'utf-8');
    console.log("Parsing single GPX file...");

    const trkRegex = /<trk>[\s\S]*?<\/trk>/g;
    let match;
    while ((match = trkRegex.exec(gpxData)) !== null) {
        const trkContent = match[0];
        const nameMatch = trkContent.match(/<name>(.*?)<\/name>/);
        const name = nameMatch ? nameMatch[1].replace('<![CDATA[', '').replace(']]>', '').trim() : "Unknown";

        const descMatch = trkContent.match(/<desc>([\s\S]*?)<\/desc>/);
        let start = "Unknown", finish = "Unknown", date = "Unknown";
        if (descMatch) {
            const desc = descMatch[1].replace('<![CDATA[', '').replace(']]>', '');
            const lines = desc.split('\n');
            if (lines[0]) {
                const cities = lines[0].split(/\s*>\s*/);
                start = cities[0]?.trim() || "Unknown";
                finish = cities[1]?.trim() || "Unknown";
            }
            if (lines[1]) date = lines[1].trim();
        }

        const points = parsePoints(trkContent);
        if (points.length > 0) stages.push({ name, start, finish, date, points });
    }
} else {
    // Multi-file mode (Giro)
    console.log("Parsing multi-file GPX directory...");
    for (const stageInfo of config.stages) {
        const prefix = config.gpxPrefix || 'stage-';
        const gpxFile = path.join(config.gpxDir, `${prefix}${stageInfo.num}-route.gpx`);
        if (!fs.existsSync(gpxFile)) {
            console.warn(`  [WARN] Missing GPX: ${gpxFile}`);
            continue;
        }
        const gpxContent = fs.readFileSync(gpxFile, 'utf-8');
        const points = parsePoints(gpxContent);
        if (points.length > 0) {
            stages.push({
                name: stageInfo.name,
                start: stageInfo.start,
                finish: stageInfo.finish,
                date: stageInfo.date,
                points,
            });
        } else {
            console.warn(`  [WARN] No points in ${gpxFile}`);
        }
    }
}

console.log(`Found ${stages.length} stages.`);

// --- Build all stages ---
const GLOBAL_LAT = config.globalLat;
const GLOBAL_LON = config.globalLon;
const rad = Math.PI / 180;
const m_per_lat = 111320.0;
const m_per_lon = 111320.0 * Math.cos(GLOBAL_LAT * rad);

const allStagesData = [];
for (const stage of stages) {
    let sumLat = 0, sumLon = 0;
    for (const pt of stage.points) { sumLat += pt.lat; sumLon += pt.lon; }
    const latCenter = sumLat / stage.points.length;
    const lonCenter = sumLon / stage.points.length;

    const global_lx = (lonCenter - GLOBAL_LON) * m_per_lon;
    const global_ly = (latCenter - GLOBAL_LAT) * m_per_lat;

    let totalDist = 0;
    let lastCoord = null;
    let profilePoints = [];
    for (const pt of stage.points) {
        if (lastCoord) totalDist += haversineDistance(lastCoord.lat, lastCoord.lon, pt.lat, pt.lon);
        const lx = (pt.lon - lonCenter) * m_per_lon;
        const ly = (pt.lat - latCenter) * m_per_lat;
        profilePoints.push({ dist: totalDist, ele: pt.ele, lx, ly });
        lastCoord = pt;
    }

    const maxEle = profilePoints.reduce((max, p) => Math.max(max, p.ele), 0);
    const minEle = profilePoints.reduce((min, p) => Math.min(min, p.ele), 9999);

    // Sparkline (60 points)
    const sparkline = new Float32Array(60);
    for (let i = 0; i < 60; i++) {
        const targetDist = (i / 59) * totalDist;
        let p = profilePoints[0];
        for (let j = 0; j < profilePoints.length; j++) {
            if (profilePoints[j].dist >= targetDist) { p = profilePoints[j]; break; }
        }
        sparkline[i] = p.ele;
    }

    // Vertices/Indices
    const vertices = [];
    const indices = [];
    let r_v_offset = 0;
    const n = profilePoints.length;
    for (let j = 0; j < n; j++) {
        const p = profilePoints[j];
        const pr = j > 0 ? profilePoints[j - 1] : p;
        const nx = j < n - 1 ? profilePoints[j + 1] : p;

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

    allStagesData.push({
        name: stage.name, start: stage.start, finish: stage.finish, date: stage.date,
        totalDist, maxEle, minEle, global_lx, global_ly,
        sparkline,
        vertices: new Float32Array(vertices),
        indices: new Uint32Array(indices),
    });
}

// --- Binary Format ---
let totalSize = 4; // num_stages
for (const s of allStagesData) {
    totalSize += 4 + Buffer.from(s.name, 'utf8').length;
    totalSize += 4 + Buffer.from(s.start, 'utf8').length;
    totalSize += 4 + Buffer.from(s.finish, 'utf8').length;
    totalSize += 4 + Buffer.from(s.date, 'utf8').length;
    totalSize += 4 + 4 + 4 + (60 * 4); // dist, maxEle, minEle, sparkline
    totalSize += 4 + 4; // global_lx, global_ly
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
    finalBuf.writeFloatLE(s.global_lx, offset); offset += 4;
    finalBuf.writeFloatLE(s.global_ly, offset); offset += 4;

    for (let i = 0; i < 60; i++) { finalBuf.writeFloatLE(s.sparkline[i], offset); offset += 4; }

    finalBuf.writeUInt32LE(s.vertices.length, offset); offset += 4;
    finalBuf.writeUInt32LE(s.indices.length, offset); offset += 4;
    Buffer.from(s.vertices.buffer).copy(finalBuf, offset); offset += s.vertices.byteLength;
    Buffer.from(s.indices.buffer).copy(finalBuf, offset); offset += s.indices.byteLength;
}

const zlib = require('zlib');
const compressedBuf = zlib.gzipSync(finalBuf);
fs.writeFileSync(binPath, compressedBuf);
console.log(`Saved ${allStagesData.length} stages to ${binPath}`);
console.log(`  Original: ${(totalSize / 1024 / 1024).toFixed(2)} MB, Compressed: ${(compressedBuf.length / 1024 / 1024).toFixed(2)} MB`);
