use d2r_core::engine::item_parallel::ParallelItemEngine;
use d2r_core::item::HuffmanTree;
use std::sync::Arc;
use std::fs;
use std::time::Instant;

fn main() {
    let huffman = Arc::new(HuffmanTree::new());
    
    // Load a sample savegame
    let save_path = "tests/fixtures/savegames/original/amazon_authority_runeword.d2s";
    let bytes = fs::read(save_path).expect("Failed to read test fixture");
    
    // Duplicate the buffer to create a large workload
    let iterations = 10;
    let mut large_buffer = Vec::new();
    for _ in 0..iterations {
        large_buffer.extend_from_slice(&bytes);
    }
    
    println!("Benchmarking with {}x savegame size ({} bytes)", iterations, large_buffer.len());
    
    let engine = ParallelItemEngine::new(huffman.clone(), true);

    // Warm up
    let _ = engine.deserialize_all(&large_buffer);

    // Measure Parallel (Rayon default)
    let start = Instant::now();
    let results_parallel = engine.deserialize_all(&large_buffer);
    let duration_parallel = start.elapsed();
    println!("Parallel: {:?} (found {} items)", duration_parallel, results_parallel.len());

    // Measure Serial (Force Rayon to 1 thread if possible, or just mock it)
    // Rayon doesn't easily allow per-call thread limit without a threadpool.
    // For a simple manual bench, we can just compare it with a serial loop if we want,
    // but the engine itself is hardcoded to use par_iter().
    
    // Alternative: create a thread pool with 1 thread for serial measurement
    let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    let start = Instant::now();
    let results_serial = pool.install(|| engine.deserialize_all(&large_buffer));
    let duration_serial = start.elapsed();
    println!("Serial (1 thread): {:?} (found {} items)", duration_serial, results_serial.len());

    if duration_parallel < duration_serial {
        let speedup = duration_serial.as_secs_f64() / duration_parallel.as_secs_f64();
        println!("Speedup: {:.2}x", speedup);
    } else {
        println!("No speedup detected (Parallel was slower or equal)");
    }
}
