//! Fire-and-forget background jobs.
//!
//! Native: a thread, exactly as `std::thread::spawn` (which is what every
//! caller used before). The browser build (`web/`) has no threads —
//! `std::thread::spawn` panics there — so the job goes to rayon's one-thread
//! pool, which the frame loop drains (`webmain::pump_rayon`): it still runs,
//! on the UI thread, between frames. Only for jobs that END: a loop waiting
//! on a channel would never give the thread back.

pub fn spawn<F: FnOnce() + Send + 'static>(f: F) {
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(f);
    #[cfg(target_arch = "wasm32")]
    rayon::spawn(f);
}
