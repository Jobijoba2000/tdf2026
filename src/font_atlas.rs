/// font_atlas.rs — équivalent de fontAtlas.js dans atlas-webgpu
/// Rasterise les glyphes depuis un fichier TTF sur CPU (via fontdue),
/// les pack dans une texture atlas, et retourne les métriques UV par caractère.

use std::collections::HashMap;

const FONT_SIZE: f32 = 48.0;
const PADDING: u32 = 3;

#[derive(Clone, Debug)]
pub struct GlyphMetrics {
    pub advance: f32,   // largeur d'avancement (en px à FONT_SIZE)
    pub width: u32,     // largeur du quad (avec padding)
    pub height: f32,    // hauteur réelle du glyphe
    pub ox: f32,        // offset x (xmin)
    pub u0: f32, pub v0: f32,
    pub u1: f32, pub v1: f32,
}

pub struct FontAtlas {
    pub metrics: HashMap<char, GlyphMetrics>,
    pub font_size: f32,
    pub tex_size: u32,
    /// données RGBA CPU (pour upload GPU)
    pub rgba_data: Vec<u8>,
}

const CHARACTERS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789àâéèêëîïôûùçÀÂÉÈÊËÎÏÔÛÙÇñÑíÍáÁóÓúÚ-.,\\'()?!:;/@#$%^&*=_+[]{}<>|•●";

impl FontAtlas {
    pub fn from_bytes(font_data: &[u8]) -> Option<Self> {
        let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default()).ok()?;

        // --- Passe 1 : mesurer chaque glyphe ---
        let row_height = (FONT_SIZE * 1.4) as u32 + PADDING * 2;
        let chars: Vec<char> = CHARACTERS.chars().collect();

        let mut char_widths: HashMap<char, u32> = HashMap::new();
        let mut char_advances: HashMap<char, f32> = HashMap::new();

        for &c in &chars {
            let (metrics, _) = font.rasterize(c, FONT_SIZE);
            let w = (metrics.width as u32) + PADDING * 2;
            char_widths.insert(c, w);
            char_advances.insert(c, metrics.advance_width);
        }

        // --- Taille de l'atlas ---
        // Forcer 1024x1024 pour être large et éviter les débordements
        let tex_size = 1024u32;

        // --- Passe 2 : rasteriser et packer ---
        let mut rgba = vec![0u8; (tex_size * tex_size * 4) as usize];
        let mut metrics_map: HashMap<char, GlyphMetrics> = HashMap::new();

        let mut cx: u32 = 0;
        let mut cy: u32 = 0;

        let line_metrics = font.horizontal_line_metrics(FONT_SIZE).unwrap_or(fontdue::LineMetrics {
            ascent: FONT_SIZE * 0.8, descent: -FONT_SIZE * 0.2, line_gap: 0.0, new_line_size: FONT_SIZE * 1.2,
        });

        for &c in &chars {
            let (gm, bitmap) = font.rasterize(c, FONT_SIZE);
            let w = char_widths[&c];

            if cx + w > tex_size {
                cx = 0;
                cy += row_height + 4;
            }
            if cy + row_height > tex_size { break; }

            // L'emplacement dans l'atlas doit tenir compte du baseline
            let bx = cx + PADDING;
            // Y dans l'atlas : on aligne sur le baseline
            let by = cy + (line_metrics.ascent - gm.bounds.ymin - gm.bounds.height) as u32;

            for row in 0..gm.height {
                for col in 0..gm.width {
                    let alpha = bitmap[row * gm.width + col];
                    let px = bx + col as u32;
                    let py = by + row as u32;
                    if px < tex_size && py < tex_size {
                        let idx = ((py * tex_size + px) * 4) as usize;
                        rgba[idx] = 255; rgba[idx+1] = 255; rgba[idx+2] = 255; 
                        rgba[idx+3] = alpha;
                    }
                }
            }

            // UVs
            let u0 = cx as f32 / tex_size as f32;
            let v0 = cy as f32 / tex_size as f32;
            let u1 = (cx + w) as f32 / tex_size as f32;
            let v1 = (cy + row_height) as f32 / tex_size as f32;

            metrics_map.insert(c, GlyphMetrics {
                advance: char_advances.get(&c).cloned().unwrap_or(0.0),
                width: w,
                height: gm.height as f32,
                ox: gm.bounds.xmin,
                u0, v0, u1, v1,
            });

            cx += w;
        }

        Some(FontAtlas {
            metrics: metrics_map,
            font_size: FONT_SIZE,
            tex_size,
            rgba_data: rgba,
        })
    }

    pub fn get_text_geometry(&self, text: &str) -> (Vec<f32>, Vec<f32>) {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();

        let mut cur_x = 0.0f32;
        let row_h = self.font_size * 1.4; // Hauteur totale du rang dans l'atlas

        // Trouver le ox du premier caractère pour aligner parfaitement à gauche
        let mut first_ox = 0.0;
        if let Some(first_char) = text.chars().next() {
            if let Some(m) = self.metrics.get(&first_char).or_else(|| self.metrics.get(&' ')) {
                first_ox = m.ox;
            }
        }

        for c in text.chars() {
            let m = match self.metrics.get(&c).or_else(|| self.metrics.get(&' ')) {
                Some(m) => m,
                None => continue,
            };

            let x0 = cur_x + m.ox - first_ox;
            let x1 = x0 + (m.width as f32);
            
            let y0 = -row_h / 2.0;
            let y1 =  row_h / 2.0;

            positions.extend_from_slice(&[x0, y0, x1, y0, x0, y1]);
            uvs.extend_from_slice(&[m.u0, m.v0, m.u1, m.v0, m.u0, m.v1]);
            positions.extend_from_slice(&[x0, y1, x1, y0, x1, y1]);
            uvs.extend_from_slice(&[m.u0, m.v1, m.u1, m.v0, m.u1, m.v1]);

            cur_x += m.advance;
        }

        (positions, uvs)
    }


}
