//! Standalone hand-off throughput measurement; no UI timing claims.
use guiraffe_chat::stream::{PushError, channel};
fn main() {
    let (p, c) = channel(64);
    let start = std::time::Instant::now();
    let worker = std::thread::spawn(move || {
        for _ in 0..100_000 {
            let mut chunk = "token ".repeat(16);
            loop {
                match p.push(chunk) {
                    Ok(()) => break,
                    Err(PushError::Full(value)) => {
                        chunk = value;
                        std::hint::spin_loop();
                    }
                    Err(e) => panic!("{e:?}"),
                }
            }
        }
    });
    let mut bytes = 0;
    loop {
        bytes += c.drain(8).len();
        if c.is_finished() {
            break;
        }
        std::hint::spin_loop();
    }
    worker.join().unwrap_or_else(|_| panic!("producer failed"));
    println!("100000 chunks / {bytes} bytes, {:?}", start.elapsed());
}
