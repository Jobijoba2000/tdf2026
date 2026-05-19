const fs = require('fs');
const path = require('path');

const RACE_METAS = {
    tdf: {
        id: "tdf",
        name: "Tour de France 2026",
        color: [1.0, 0.85, 0.0, 1.0],
        map_center_lat: 46.5,
        map_center_lon: 2.5,
    },
    giro: {
        id: "giro",
        name: "Giro d'Italia 2026",
        color: [1.0, 0.6, 0.72, 1.0],
        map_center_lat: 42.5,
        map_center_lon: 12.5,
    },
    vuelta: {
        id: "vuelta",
        name: "Vuelta a España 2026",
        color: [1.0, 0.0, 0.0, 1.0],
        map_center_lat: 40.0,
        map_center_lon: -3.5,
    },
};

const args = process.argv.slice(2);
const raceArgIdx = args.indexOf('--race');
if (raceArgIdx === -1 || !args[raceArgIdx + 1]) {
    // Generate all
    console.log("No --race specified, generating meta.json for all races...");
    for (const [id, meta] of Object.entries(RACE_METAS)) {
        const outDir = path.join(__dirname, `../data/races/${id}`);
        if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });
        fs.writeFileSync(path.join(outDir, 'meta.json'), JSON.stringify(meta, null, 2), 'utf8');
        console.log(`  Written: data/races/${id}/meta.json`);
    }
} else {
    const raceId = args[raceArgIdx + 1];
    if (!RACE_METAS[raceId]) {
        console.error(`Unknown race: "${raceId}". Available: ${Object.keys(RACE_METAS).join(', ')}`);
        process.exit(1);
    }
    const outDir = path.join(__dirname, `../data/races/${raceId}`);
    if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true });
    fs.writeFileSync(path.join(outDir, 'meta.json'), JSON.stringify(RACE_METAS[raceId], null, 2), 'utf8');
    console.log(`Written: data/races/${raceId}/meta.json`);
}
