use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        RwLock,
    },
    time::Instant,
};

use gtk::gdk;

#[derive(Debug)]
pub(crate) struct FrameCollection {
    frames: RwLock<Box<[Option<gdk::MemoryTexture>]>>,
    creation_instant: Instant,
    last_use_time: AtomicU64,
}

impl FrameCollection {
    pub(crate) fn new(frame_count: usize) -> Self {
        Self {
            frames: RwLock::new(vec![None; frame_count].into_boxed_slice()),
            creation_instant: Instant::now(),
            last_use_time: AtomicU64::new(0),
        }
    }

    pub(crate) fn frame(&self, index: usize) -> Option<gdk::MemoryTexture> {
        self.update_last_use_time();
        let frames = self.frames.read().unwrap();
        frames[index].clone()
    }

    pub(super) fn set_frame(&self, index: usize, frame: gdk::MemoryTexture) {
        let mut frames = self.frames.write().unwrap();
        let old = frames[index].replace(frame);
        if old.is_some() {
            dbg!("unnecessary render");
        }
    }

    pub(super) fn lock_frames(
        &self,
    ) -> std::sync::RwLockReadGuard<'_, Box<[Option<gdk::MemoryTexture>]>> {
        self.update_last_use_time();
        self.frames.read().unwrap()
    }

    pub(super) fn seconds_since_last_use(&self) -> u64 {
        let since_last_use = self.last_use_time.load(Ordering::Relaxed);
        let since_creation = self.creation_instant.elapsed().as_secs();
        since_creation - since_last_use
    }

    fn update_last_use_time(&self) {
        let elapsed = self.creation_instant.elapsed().as_secs();
        self.last_use_time.store(elapsed, Ordering::Relaxed);
    }
}

trait IsSendSync: Send + Sync {}
impl IsSendSync for FrameCollection {}
