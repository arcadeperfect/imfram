//! Thumbnails as textures, loaded off the UI thread by the app's loader.
//!
//! Each frame the view says what it would like (`want`), most urgent
//! first; `flush` then hands the workers exactly that list, so anything
//! scrolled past before a worker reached it is never loaded. Finished
//! images upload a few per frame (no hitch when a page of them lands) and
//! the least recently drawn are dropped past a texel budget — never one
//! drawn this frame.
//!
//! Sizes come in power-of-two buckets: a tile asks for the bucket at or
//! above its size in pixels, and while a bigger one loads any bucket
//! already here is drawn instead (zooming in sharpens, it never blanks).
//!
//! In a page (wasm, no threads) the loader runs on the UI thread, one job
//! per flush.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

/// What a worker is asked for: `src` (the app's key: a path, an id) at
/// `frame`, no bigger than `max_px` on its long side.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Request<S> {
    pub src: S,
    pub frame: u32,
    pub max_px: u32,
}

pub type Loader<S> = dyn Fn(&Request<S>) -> Result<ColorImage, String> + Send + Sync;

pub const MIN_PX: u32 = 64;
pub const MAX_PX: u32 = 1024;

/// The bucket for a tile `px` pixels across.
pub fn bucket(px: f32) -> u32 {
    (px.max(1.0).ceil() as u32).next_power_of_two().clamp(MIN_PX, MAX_PX)
}

/// A texture to draw and its pixel size.
#[derive(Clone)]
pub struct Shown {
    pub tex: TextureHandle,
    pub size: [usize; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub textures: usize,
    pub texels: usize,
    pub queued: usize,
    pub running: usize,
    pub waiting_upload: usize,
    pub failed: usize,
    /// Loads finished since the cache was made.
    pub loaded: usize,
}

struct Queue<S> {
    jobs: Vec<(Request<S>, f32)>,
    running: HashSet<Request<S>>,
    quit: bool,
}

struct Shared<S> {
    q: Mutex<Queue<S>>,
    cv: Condvar,
}

type Done<S> = (Request<S>, Result<ColorImage, String>);

pub struct Thumbs<S: Clone + Eq + Hash + Send + 'static> {
    loader: Arc<Loader<S>>,
    shared: Arc<Shared<S>>,
    tx: Sender<Done<S>>,
    rx: Receiver<Done<S>>,
    threads: usize,
    started: bool,
    ready: HashMap<Request<S>, (Shown, u64)>,
    /// Buckets ready per (src, frame), for the stand-in while one loads.
    buckets: HashMap<(S, u32), Vec<u32>>,
    failed: HashMap<(S, u32), String>,
    uploads: VecDeque<Done<S>>,
    wanted: Vec<(Request<S>, f32)>,
    frame: u64,
    texels: usize,
    loaded: usize,
    /// Uploads per flush.
    pub upload_budget: usize,
    /// Texels kept before the least recently drawn go (default 48M ≈ 190 MB).
    pub texel_budget: usize,
    pub options: TextureOptions,
}

impl<S: Clone + Eq + Hash + Send + 'static> Thumbs<S> {
    /// `threads` workers (0: the UI thread, as in a page) running `loader`.
    pub fn new(threads: usize, loader: impl Fn(&Request<S>) -> Result<ColorImage, String> + Send + Sync + 'static) -> Self {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared { q: Mutex::new(Queue { jobs: vec![], running: HashSet::new(), quit: false }), cv: Condvar::new() });
        #[cfg(target_arch = "wasm32")]
        let threads = 0;
        Self {
            loader: Arc::new(loader),
            shared,
            tx,
            rx,
            threads,
            started: false,
            ready: HashMap::new(),
            buckets: HashMap::new(),
            failed: HashMap::new(),
            uploads: VecDeque::new(),
            wanted: vec![],
            frame: 0,
            texels: 0,
            loaded: 0,
            upload_budget: 6,
            texel_budget: 48 << 20,
            options: TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR },
        }
    }

    /// The best texture for `src` at `frame` now, asking for the `max_px`
    /// bucket at `priority` (higher first) if it is not here yet.
    pub fn want(&mut self, src: &S, frame: u32, max_px: u32, priority: f32) -> Option<Shown> {
        let req = Request { src: src.clone(), frame, max_px };
        if let Some((shown, used)) = self.ready.get_mut(&req) {
            *used = self.frame;
            return Some(shown.clone());
        }
        let key = (src.clone(), frame);
        if !self.failed.contains_key(&key) {
            self.wanted.push((req, priority));
        }
        // A stand-in: the biggest other bucket already here.
        let px = *self.buckets.get(&key)?.iter().max()?;
        let (shown, used) = self.ready.get_mut(&Request { src: src.clone(), frame, max_px: px })?;
        *used = self.frame;
        Some(shown.clone())
    }

    /// Whether `src` at `frame` is here in some bucket (no request made).
    pub fn has(&self, src: &S, frame: u32) -> bool {
        self.buckets.get(&(src.clone(), frame)).is_some_and(|b| !b.is_empty())
    }

    /// Why `src` at `frame` has no thumbnail (the loader's error).
    pub fn failed(&self, src: &S, frame: u32) -> Option<&str> {
        self.failed.get(&(src.clone(), frame)).map(String::as_str)
    }

    /// Drop everything held for `src` (its file changed); asked again next frame.
    pub fn forget(&mut self, src: &S) {
        let gone: Vec<_> = self.ready.keys().filter(|r| &r.src == src).cloned().collect();
        for r in gone {
            self.drop_ready(&r);
        }
        self.failed.retain(|(s, _), _| s != src);
    }

    pub fn stats(&self) -> Stats {
        let q = self.shared.q.lock().unwrap();
        Stats {
            textures: self.ready.len(),
            texels: self.texels,
            queued: q.jobs.len(),
            running: q.running.len(),
            waiting_upload: self.uploads.len(),
            failed: self.failed.len(),
            loaded: self.loaded,
        }
    }

    /// End of the frame: take finished loads, upload some, give the
    /// workers this frame's wants, drop old textures past the budget.
    pub fn flush(&mut self, ctx: &Context) {
        self.start(ctx);
        while let Ok(done) = self.rx.try_recv() {
            self.uploads.push_back(done);
        }
        if self.threads == 0 {
            self.run_one_inline();
        }
        for _ in 0..self.upload_budget {
            let Some((req, res)) = self.uploads.pop_front() else { break };
            self.land(ctx, req, res);
        }
        if !self.uploads.is_empty() {
            ctx.request_repaint();
        }
        self.hand_out();
        self.evict();
        self.frame += 1;
    }

    fn land(&mut self, ctx: &Context, req: Request<S>, res: Result<ColorImage, String>) {
        self.loaded += 1;
        let key = (req.src.clone(), req.frame);
        match res {
            Ok(img) => {
                let size = img.size;
                let tex = ctx.load_texture(format!("imfram.thumb.{}", self.loaded), img, self.options);
                self.texels += size[0] * size[1];
                self.buckets.entry(key).or_default().push(req.max_px);
                self.ready.insert(req, (Shown { tex, size }, self.frame));
            }
            Err(e) => {
                self.failed.insert(key, e);
            }
        }
    }

    /// The queue becomes this frame's wants, minus what is running or
    /// already waiting to upload; the rest (scrolled away) is dropped.
    fn hand_out(&mut self) {
        let mut wanted = std::mem::take(&mut self.wanted);
        let waiting: HashSet<&Request<S>> = self.uploads.iter().map(|(r, _)| r).collect();
        let mut q = self.shared.q.lock().unwrap();
        wanted.retain(|(r, _)| !q.running.contains(r) && !waiting.contains(r));
        // Lowest priority first: workers pop from the end.
        wanted.sort_by(|a, b| a.1.total_cmp(&b.1));
        wanted.dedup_by(|a, b| a.0 == b.0);
        let any = !wanted.is_empty();
        q.jobs = wanted;
        drop(q);
        if any {
            self.shared.cv.notify_all();
        }
    }

    fn evict(&mut self) {
        if self.texels <= self.texel_budget {
            return;
        }
        let mut old: Vec<(u64, Request<S>)> = self.ready.iter().filter(|(_, (_, used))| *used < self.frame).map(|(r, (_, used))| (*used, r.clone())).collect();
        old.sort_by_key(|(used, _)| *used);
        for (_, r) in old {
            if self.texels <= self.texel_budget {
                break;
            }
            self.drop_ready(&r);
        }
    }

    fn drop_ready(&mut self, r: &Request<S>) {
        if let Some((shown, _)) = self.ready.remove(r) {
            self.texels -= shown.size[0] * shown.size[1];
            let key = (r.src.clone(), r.frame);
            if let Some(b) = self.buckets.get_mut(&key) {
                b.retain(|&px| px != r.max_px);
                if b.is_empty() {
                    self.buckets.remove(&key);
                }
            }
        }
    }

    fn run_one_inline(&mut self) {
        let mut wanted = std::mem::take(&mut self.wanted);
        wanted.sort_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((req, _)) = wanted.pop() {
            let res = (self.loader)(&req);
            self.uploads.push_back((req, res));
        }
        self.wanted = wanted;
    }

    fn start(&mut self, ctx: &Context) {
        if self.started || self.threads == 0 {
            return;
        }
        self.started = true;
        for n in 0..self.threads {
            let (shared, loader, tx, ctx) = (self.shared.clone(), self.loader.clone(), self.tx.clone(), ctx.clone());
            let _ = std::thread::Builder::new().name(format!("imfram-thumb-{n}")).spawn(move || work(&shared, &*loader, &tx, &ctx));
        }
    }
}

impl<S: Clone + Eq + Hash + Send + 'static> Drop for Thumbs<S> {
    fn drop(&mut self) {
        self.shared.q.lock().unwrap().quit = true;
        self.shared.cv.notify_all();
    }
}

fn work<S: Clone + Eq + Hash>(shared: &Shared<S>, loader: &Loader<S>, tx: &Sender<Done<S>>, ctx: &Context) {
    loop {
        let req = {
            let mut q = shared.q.lock().unwrap();
            while q.jobs.is_empty() && !q.quit {
                q = shared.cv.wait(q).unwrap();
            }
            if q.quit {
                return;
            }
            let (req, _) = q.jobs.pop().expect("not empty");
            q.running.insert(req.clone());
            req
        };
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(&req))).unwrap_or_else(|_| Err("the loader panicked".into()));
        // Sent before it stops counting as running, so `hand_out` never
        // sees it in neither place and asks for it twice.
        if tx.send((req.clone(), res)).is_err() {
            return;
        }
        shared.q.lock().unwrap().running.remove(&req);
        ctx.request_repaint();
    }
}
