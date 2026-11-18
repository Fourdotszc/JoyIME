#[path = "../src/caret.rs"]
mod caret;

use std::{thread, time::{Duration, Instant}};

fn main() {
    let mut tracker = caret::Tracker::new();
    loop {
        let started = Instant::now();
        match tracker.poll() {
            Some(focus) => println!(
                "editable={} caret={:?} element={:?} source={} process_id={} elapsed_ms={:.3}",
                focus.editable,
                focus.caret,
                focus.element,
                focus.source,
                focus.process_id,
                started.elapsed().as_secs_f64() * 1_000.0,
            ),
            None => println!(
                "editable=false caret=None element=None source=none process_id=0 elapsed_ms={:.3}",
                started.elapsed().as_secs_f64() * 1_000.0,
            ),
        }
        thread::sleep(Duration::from_millis(500));
    }
}
