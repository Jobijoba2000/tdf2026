use std::fs::File;
use std::io::Read;

#[derive(Debug)]
struct Stage {
    name: String,
    max_dist: f32,
    max_ele: f32,
}

fn read_string(f: &mut File) -> String {
    let mut len_buf = [0u8; 4];
    f.read_exact(&mut len_buf).unwrap();
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut str_buf = vec![0u8; len];
    f.read_exact(&mut str_buf).unwrap();
    String::from_utf8(str_buf).unwrap()
}

fn main() {
    let mut f = File::open("../data/profile.bin").unwrap();
    let mut num_stages_buf = [0u8; 4];
    f.read_exact(&mut num_stages_buf).unwrap();
    let num_stages = u32::from_le_bytes(num_stages_buf);
    
    let mut stages = Vec::new();
    for _ in 0..num_stages {
        let name = read_string(&mut f);
        let _start = read_string(&mut f);
        let _finish = read_string(&mut f);
        let _date = read_string(&mut f);
        
        let mut float_buf = [0u8; 4];
        f.read_exact(&mut float_buf).unwrap();
        let max_dist = f32::from_le_bytes(float_buf);
        
        f.read_exact(&mut float_buf).unwrap();
        let max_ele = f32::from_le_bytes(float_buf);
        
        f.read_exact(&mut float_buf).unwrap();
        let _min_ele = f32::from_le_bytes(float_buf);
        
        // skip sparkline
        let mut sparkline_buf = vec![0u8; 60 * 4];
        f.read_exact(&mut sparkline_buf).unwrap();
        
        // skip vertices
        let mut num_v_buf = [0u8; 4];
        f.read_exact(&mut num_v_buf).unwrap();
        let num_v = u32::from_le_bytes(num_v_buf) as usize;
        let mut v_buf = vec![0u8; num_v * 4];
        f.read_exact(&mut v_buf).unwrap();
        
        // skip indices
        let mut num_i_buf = [0u8; 4];
        f.read_exact(&mut num_i_buf).unwrap();
        let num_i = u32::from_le_bytes(num_i_buf) as usize;
        let mut i_buf = vec![0u8; num_i * 4];
        f.read_exact(&mut i_buf).unwrap();
        
        // skip profile points
        let mut num_p_buf = [0u8; 4];
        f.read_exact(&mut num_p_buf).unwrap();
        let num_p = u32::from_le_bytes(num_p_buf) as usize;
        let mut p_buf = vec![0u8; num_p * 8];
        f.read_exact(&mut p_buf).unwrap();
        
        stages.push(Stage { name, max_dist, max_ele });
    }
    
    let (ref_idx, max_ratio) = stages.iter().enumerate()
        .map(|(i, s)| (i, s.max_ele / s.max_dist))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap();
        
    println!("Reference stage index: {}", ref_idx);
    println!("Reference stage name: {}", stages[ref_idx].name);
    println!("Reference stage max_ele: {}", stages[ref_idx].max_ele);
    println!("Reference stage max_dist: {}", stages[ref_idx].max_dist);
    println!("Global Max Ratio (K): {}", max_ratio);
}
